//! The board: work items in columns, run by agents, synced with trackers.
//!
//! An item is generic (a title, a body, labels, and how to run it), so the
//! board serves security reviews, fixes, research or anything else an agent
//! can do. Columns: todo -> running -> review | failed -> done. An agent
//! finishing moves its item to review (or failed); only a person moves an
//! item to done. Items imported from a tracker carry their source, and each
//! move is reported back: a comment, and the mapped state.

use crate::deck::config::{ConnectorCfg, Settings};
use crate::deck::procs::{Launch, StartOpts};
use crate::deck::trackers::{Remote, Tracker};
use crate::deck::web::App;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub type Result<T> = std::result::Result<T, String>;

pub const COLUMNS: [&str; 5] = ["todo", "running", "review", "done", "failed"];

/// The board every item belongs to until there are others.
pub const MAIN: &str = "main";

fn main_board() -> String {
    MAIN.into()
}

/// One board: a name, and the trackers whose items it imports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoardMeta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Connectors that import into this board (others import into "main").
    #[serde(default)]
    pub connectors: Vec<String>,
}

const PROMPT: &str = "Work on this task.\n\nTitle: {title}\nKey: {key}\nLink: {url}\n\n{body}\n\n\
When you are done, summarise what you did, what you changed, and anything left to do.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub connector: String,
    pub key: String,
    pub url: String,
    /// Its state in the tracker when last seen or set.
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub ts: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: u64,
    /// The board it is on.
    #[serde(default = "main_board")]
    pub board: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub labels: Vec<String>,
    pub column: String,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub runner: Option<String>,
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default)]
    pub proc_id: Option<u64>,
    #[serde(default)]
    pub procs: Vec<u64>,
    #[serde(default)]
    pub result: Option<String>,
    pub created: u64,
    pub updated: u64,
    #[serde(default)]
    pub notes: Vec<Note>,
}

/// Fields a person can set on an item.
#[derive(Debug, Default, Deserialize)]
pub struct Patch {
    /// Move it to another board (or, on create, put it there).
    pub board: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    pub labels: Option<Vec<String>>,
    pub column: Option<String>,
    pub profile: Option<String>,
    pub runner: Option<String>,
    pub node: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Data {
    next: u64,
    items: Vec<Item>,
    #[serde(default)]
    boards: Vec<BoardMeta>,
}

impl Data {
    /// The boards, "main" always among them.
    fn boards(&self) -> Vec<BoardMeta> {
        let mut v = self.boards.clone();
        if !v.iter().any(|b| b.id == MAIN) {
            v.insert(
                0,
                BoardMeta {
                    id: MAIN.into(),
                    name: "Board".into(),
                    description: String::new(),
                    connectors: vec![],
                },
            );
        }
        v
    }
    fn has_board(&self, id: &str) -> bool {
        id == MAIN || self.boards.iter().any(|b| b.id == id)
    }
    /// The board a connector imports into.
    fn board_for(&self, connector: &str) -> String {
        self.boards
            .iter()
            .find(|b| b.connectors.iter().any(|c| c == connector))
            .map(|b| b.id.clone())
            .unwrap_or_else(main_board)
    }
}

/// A board id from its name: lowercase letters, digits and dashes.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out
        .trim_end_matches('-')
        .chars()
        .take(40)
        .collect::<String>();
    if out.is_empty() {
        "board".into()
    } else {
        out
    }
}

pub struct Board {
    path: PathBuf,
    data: Mutex<Data>,
    /// When each connector last imported (for poll_minutes).
    polled: Mutex<HashMap<String, u64>>,
    /// One reconcile at a time.
    ticking: tokio::sync::Mutex<()>,
}

fn now() -> u64 {
    crate::deck::runs::now()
}

fn clip(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(n).collect::<String>())
    }
}

impl Board {
    pub fn open(path: PathBuf) -> Board {
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Board {
            path,
            data: Mutex::new(data),
            polled: Mutex::new(HashMap::new()),
            ticking: tokio::sync::Mutex::new(()),
        }
    }

    fn save(&self, d: &Data) {
        if let Ok(text) = serde_json::to_string_pretty(d) {
            if let Some(dir) = self.path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = crate::deck::confedit::write_atomic(&self.path, &text);
        }
    }

    pub fn items(&self) -> Vec<Item> {
        self.data.lock().unwrap().items.clone()
    }

    pub fn boards(&self) -> Vec<BoardMeta> {
        self.data.lock().unwrap().boards()
    }

    /// A new board; returns its id.
    pub fn add_board(&self, name: &str, description: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 80 {
            return Err("a board needs a name (up to 80 characters)".into());
        }
        let mut d = self.data.lock().unwrap();
        if d.boards().iter().any(|b| b.name.eq_ignore_ascii_case(name)) {
            return Err(format!("there is already a board named '{}'", name));
        }
        let base = slug(name);
        let mut id = base.clone();
        let mut n = 1;
        while d.has_board(&id) {
            n += 1;
            id = format!("{}-{}", base, n);
        }
        d.boards.push(BoardMeta {
            id: id.clone(),
            name: name.into(),
            description: description.trim().into(),
            connectors: vec![],
        });
        self.save(&d);
        Ok(id)
    }

    /// Rename a board, describe it, or choose the trackers it imports.
    pub fn update_board(
        &self,
        id: &str,
        name: Option<String>,
        description: Option<String>,
        connectors: Option<Vec<String>>,
    ) -> Result<BoardMeta> {
        let mut d = self.data.lock().unwrap();
        if !d.has_board(id) {
            return Err("no such board".into());
        }
        if let Some(n) = &name {
            let n = n.trim();
            if n.is_empty() || n.chars().count() > 80 {
                return Err("a board needs a name (up to 80 characters)".into());
            }
            if d.boards()
                .iter()
                .any(|b| b.id != id && b.name.eq_ignore_ascii_case(n))
            {
                return Err(format!("there is already a board named '{}'", n));
            }
        }
        // "main" is implicit until it is first changed.
        if !d.boards.iter().any(|b| b.id == id) {
            let m = d.boards().into_iter().find(|b| b.id == id).unwrap();
            d.boards.insert(0, m);
        }
        if let Some(cs) = &connectors {
            // A connector imports into one board.
            for b in d.boards.iter_mut().filter(|b| b.id != id) {
                b.connectors.retain(|c| !cs.contains(c));
            }
        }
        let b = d.boards.iter_mut().find(|b| b.id == id).unwrap();
        if let Some(n) = name {
            b.name = n.trim().into();
        }
        if let Some(x) = description {
            b.description = x.trim().into();
        }
        if let Some(cs) = connectors {
            b.connectors = cs;
        }
        let out = b.clone();
        self.save(&d);
        Ok(out)
    }

    /// Delete a board. Its items move to `move_to`, or are deleted with it
    /// when none is given; none may have an agent working on it.
    pub fn delete_board(&self, id: &str, move_to: Option<&str>) -> Result<usize> {
        let mut d = self.data.lock().unwrap();
        if id == MAIN {
            return Err("the first board cannot be deleted (rename it instead)".into());
        }
        if !d.has_board(id) {
            return Err("no such board".into());
        }
        if let Some(t) = move_to {
            if t == id || !d.has_board(t) {
                return Err("move the items to another existing board".into());
            }
        }
        if d.items
            .iter()
            .any(|i| i.board == id && i.column == "running")
        {
            return Err("an agent is working on an item of this board; stop it first".into());
        }
        let mut n = 0;
        match move_to {
            Some(t) => {
                for it in d.items.iter_mut().filter(|i| i.board == id) {
                    it.board = t.into();
                    n += 1;
                }
            }
            None => {
                let before = d.items.len();
                d.items.retain(|i| i.board != id);
                n = before - d.items.len();
            }
        }
        // Its trackers import into "main" from now on.
        d.boards.retain(|b| b.id != id);
        self.save(&d);
        Ok(n)
    }

    pub fn get(&self, id: u64) -> Option<Item> {
        self.data
            .lock()
            .unwrap()
            .items
            .iter()
            .find(|i| i.id == id)
            .cloned()
    }

    fn edit<T>(&self, id: u64, f: impl FnOnce(&mut Item) -> Result<T>) -> Result<T> {
        let mut d = self.data.lock().unwrap();
        let it = d
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or("no such item")?;
        let r = f(it)?;
        it.updated = now();
        self.save(&d);
        Ok(r)
    }

    pub fn create(&self, title: &str, p: Patch, source: Option<Source>) -> Result<u64> {
        if title.trim().is_empty() || title.len() > 500 {
            return Err("an item needs a title (up to 500 characters)".into());
        }
        let mut d = self.data.lock().unwrap();
        let board = p.board.clone().unwrap_or_else(main_board);
        if !d.has_board(&board) {
            return Err(format!("no board '{}'", board));
        }
        d.next += 1;
        let id = d.next;
        let t = now();
        let mut it = Item {
            id,
            board,
            title: title.trim().to_string(),
            body: String::new(),
            labels: vec![],
            column: "todo".into(),
            profile: None,
            runner: None,
            node: None,
            target: None,
            source,
            proc_id: None,
            procs: vec![],
            result: None,
            created: t,
            updated: t,
            notes: vec![],
        };
        apply(&mut it, p)?;
        it.column = "todo".into();
        d.items.push(it);
        self.save(&d);
        Ok(id)
    }

    pub fn update(&self, id: u64, p: Patch) -> Result<Item> {
        if let Some(b) = &p.board {
            if !self.data.lock().unwrap().has_board(b) {
                return Err(format!("no board '{}'", b));
            }
        }
        self.edit(id, |it| {
            if let Some(c) = &p.column {
                if c == "running" && it.column != "running" {
                    return Err(
                        "an item becomes running when an agent starts on it: use Run".into(),
                    );
                }
                if it.column == "running" && c != "running" {
                    return Err("an agent is working on it; stop the agent first".into());
                }
            }
            apply(it, p)?;
            Ok(it.clone())
        })
    }

    pub fn delete(&self, id: u64) -> Result<()> {
        let mut d = self.data.lock().unwrap();
        let n = d.items.len();
        d.items.retain(|i| i.id != id || i.column == "running");
        if d.items.len() == n {
            return Err("no such item, or an agent is working on it".into());
        }
        self.save(&d);
        Ok(())
    }

    pub fn note(&self, id: u64, text: String) {
        let _ = self.edit(id, |it| {
            it.notes.push(Note { ts: now(), text });
            if it.notes.len() > 50 {
                it.notes.remove(0);
            }
            Ok(())
        });
    }

    /// Add tracker items that are not on the board yet; refresh the others' text.
    fn merge(&self, connector: &str, remote: Vec<Remote>) -> (usize, usize) {
        let mut d = self.data.lock().unwrap();
        let (mut added, mut updated) = (0, 0);
        for r in remote {
            if let Some(it) = d.items.iter_mut().find(|i| {
                i.source
                    .as_ref()
                    .map(|s| s.connector == connector && s.key == r.key)
                    .unwrap_or(false)
            }) {
                if it.title != r.title || it.body != r.body {
                    it.title = r.title.clone();
                    it.body = r.body.clone();
                    it.updated = now();
                    updated += 1;
                }
                continue;
            }
            d.next += 1;
            let t = now();
            let id = d.next;
            let board = d.board_for(connector);
            d.items.push(Item {
                id,
                board,
                title: if r.title.is_empty() {
                    r.key.clone()
                } else {
                    r.title.clone()
                },
                body: r.body.clone(),
                labels: vec![],
                column: "todo".into(),
                profile: None,
                runner: None,
                node: None,
                target: None,
                source: Some(Source {
                    connector: connector.into(),
                    key: r.key,
                    url: r.url,
                    state: r.state,
                }),
                proc_id: None,
                procs: vec![],
                result: None,
                created: t,
                updated: t,
                notes: vec![Note {
                    ts: t,
                    text: format!("imported from {}", connector),
                }],
            });
            added += 1;
        }
        self.save(&d);
        (added, updated)
    }
}

fn apply(it: &mut Item, p: Patch) -> Result<()> {
    let opt = |v: Option<String>| v.map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    if let Some(b) = p.board {
        it.board = b;
    }
    if let Some(t) = p.title {
        if t.trim().is_empty() {
            return Err("the title cannot be empty".into());
        }
        it.title = t.trim().to_string();
    }
    if let Some(b) = p.body {
        if b.len() > 100_000 {
            return Err("the body is too long".into());
        }
        it.body = b;
    }
    if let Some(l) = p.labels {
        it.labels = l
            .into_iter()
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect();
    }
    if let Some(c) = p.column {
        if !COLUMNS.contains(&c.as_str()) {
            return Err(format!("column must be one of {}", COLUMNS.join(", ")));
        }
        it.column = c;
    }
    if p.profile.is_some() {
        it.profile = opt(p.profile);
    }
    if p.runner.is_some() {
        it.runner = opt(p.runner);
    }
    if p.node.is_some() {
        it.node = opt(p.node);
    }
    if p.target.is_some() {
        it.target = opt(p.target);
    }
    Ok(())
}

/// A connector with its token taken from the secret store. A secret can be
/// scoped to a connector with the profile entry `connector:<name>`.
fn tracker<'a>(app: &App, c: &'a ConnectorCfg) -> Result<Tracker<'a>> {
    let env: std::collections::BTreeMap<String, String> = [("TOKEN".to_string(), c.token.clone())]
        .into_iter()
        .collect();
    let scope = format!("connector:{}", c.name);
    let (resolved, _) = app
        .procs
        .vault()
        .resolve(&env, Some(&scope), crate::deck::nodes::LOCAL)?;
    Ok(Tracker {
        cfg: c,
        token: resolved["TOKEN"].clone(),
    })
}

/// Import from one connector now. Returns (added, updated).
pub async fn import(app: &App, connector: &str) -> Result<(usize, usize)> {
    let settings = app.settings();
    let c = settings
        .connector(connector)
        .ok_or_else(|| format!("no connector named '{}'", connector))?;
    let t = tracker(app, c)?;
    let remote = t.list().await?;
    app.polled(connector);
    Ok(app.board.merge(connector, remote))
}

/// Where and how an item runs: explicit choices, then the item's, then its
/// connector's, then the first target of the harness on this machine.
pub struct RunReq {
    pub profile: Option<String>,
    pub runner: Option<String>,
    pub node: Option<String>,
    pub target: Option<String>,
}

fn prompt_for(it: &Item, c: Option<&ConnectorCfg>) -> String {
    let tpl = c
        .and_then(|c| c.prompt.clone())
        .unwrap_or_else(|| PROMPT.to_string());
    let (key, url) = it
        .source
        .as_ref()
        .map(|s| (s.key.clone(), s.url.clone()))
        .unwrap_or_else(|| (format!("board-{}", it.id), String::new()));
    tpl.replace("{title}", &it.title)
        .replace("{key}", &key)
        .replace("{url}", &url)
        .replace("{body}", &it.body)
}

/// Start an agent on an item: it moves to running, and its tracker hears.
pub async fn run(app: &App, id: u64, req: RunReq) -> Result<u64> {
    let it = app.board.get(id).ok_or("no such item")?;
    if it.column == "running" {
        return Err("an agent is already working on it".into());
    }
    let settings = app.settings();
    let conn = it
        .source
        .as_ref()
        .and_then(|s| settings.connector(&s.connector));
    let pick = |a: Option<String>, b: &Option<String>, c: Option<String>| {
        a.filter(|x| !x.is_empty()).or_else(|| b.clone()).or(c)
    };
    let profile = pick(
        req.profile,
        &it.profile,
        conn.and_then(|c| c.profile.clone()),
    );
    let node = pick(req.node, &it.node, conn.and_then(|c| c.node.clone()))
        .unwrap_or_else(|| crate::deck::nodes::LOCAL.into());
    let target = pick(req.target, &it.target, conn.and_then(|c| c.target.clone()));
    let runner = pick(req.runner, &it.runner, None).unwrap_or_default();
    let (runner, agent) = crate::deck::web::resolve_agent(
        &settings,
        profile.as_deref(),
        &runner,
        &Default::default(),
    )
    .map_err(|e| e.message())?;
    let harness = settings
        .harnesses
        .first()
        .map(|h| h.name.clone())
        .unwrap_or_default();
    let (prompt, memory, reflect) = crate::deck::knowledge::prepare(
        &settings,
        &harness,
        &prompt_for(&it, conn),
        profile.as_deref(),
        None,
        None,
    );
    let start = StartOpts {
        agent,
        profile: profile.clone(),
        memory,
        reflect,
        ..Default::default()
    };
    let pid = if node == crate::deck::nodes::LOCAL {
        let target = match target {
            Some(t) => PathBuf::from(t),
            None => settings
                .harnesses
                .first()
                .and_then(|h| h.targets.first().cloned())
                .ok_or("no target: set one on the item, its connector, or the harness")?,
        };
        let r = settings
            .runners
            .iter()
            .find(|r| r.name == runner)
            .ok_or_else(|| format!("unknown runner '{}'", runner))?;
        app.procs.start_with(
            Launch::Local {
                runner: r,
                harness: &harness,
                prompt: &prompt,
                target: &target,
            },
            start,
        )?
    } else {
        let n = settings
            .node(&node)
            .ok_or_else(|| format!("unknown node '{}'", node))?;
        let target = target
            .or_else(|| n.targets.first().cloned())
            .ok_or_else(|| format!("no target on node '{}'", node))?;
        app.procs.start_with(
            Launch::Remote {
                node: n,
                runner: &runner,
                harness: &harness,
                prompt: &prompt,
                target: &target,
            },
            start,
        )?
    };
    app.board.edit(id, |it| {
        it.column = "running".into();
        it.proc_id = Some(pid);
        it.procs.push(pid);
        it.result = None;
        it.notes.push(Note {
            ts: now(),
            text: format!(
                "agent process #{} started{}",
                pid,
                profile
                    .as_deref()
                    .map(|p| format!(" with profile {}", p))
                    .unwrap_or_default()
            ),
        });
        Ok(())
    })?;
    if let Some(c) = conn {
        report(
            app,
            id,
            c,
            "running",
            &format!("indra started an agent on this (process #{}).", pid),
        )
        .await;
    }
    Ok(pid)
}

/// Tell the tracker: a comment, then the state mapped to `column` (if any).
async fn report(app: &App, id: u64, c: &ConnectorCfg, column: &str, comment: &str) {
    let Some(it) = app.board.get(id) else { return };
    let Some(src) = it.source else { return };
    let t = match tracker(app, c) {
        Ok(t) => t,
        Err(e) => {
            app.board
                .note(id, format!("could not reach {}: {}", c.name, e));
            return;
        }
    };
    if !comment.is_empty() {
        if let Err(e) = t.comment(&src.key, comment).await {
            app.board
                .note(id, format!("comment on {} failed: {}", src.key, e));
        }
    }
    if let Some(state) = c.states.get(column) {
        match t.transition(&src.key, state).await {
            Ok(()) => {
                let s = state.clone();
                let _ = app.board.edit(id, |it| {
                    if let Some(src) = it.source.as_mut() {
                        src.state = s.clone();
                    }
                    it.notes.push(Note {
                        ts: now(),
                        text: format!("{} moved to {}", src.key, s),
                    });
                    Ok(())
                });
            }
            Err(e) => app
                .board
                .note(id, format!("moving {} to {} failed: {}", src.key, state, e)),
        }
    }
}

/// A person moved an item: report done (and the other mapped columns) back.
pub async fn moved(app: &App, id: u64, column: &str) {
    let settings = app.settings();
    let Some(it) = app.board.get(id) else { return };
    let Some(src) = &it.source else { return };
    let Some(c) = settings.connector(&src.connector) else {
        return;
    };
    let comment = if column == "done" {
        "Accepted as done in indra."
    } else {
        ""
    };
    report(app, id, c, column, comment).await;
}

/// Keep the board moving: finished agents move their items (and report),
/// connectors with poll_minutes import, and auto_run connectors start agents
/// on new items while their node has room. Safe to call often.
pub async fn tick(app: &App) {
    let Ok(_one) = app.board.ticking.try_lock() else {
        return;
    };
    let settings = app.settings();
    // 1. Finished agents.
    for it in app
        .board
        .items()
        .into_iter()
        .filter(|i| i.column == "running")
    {
        let Some(pid) = it.proc_id else { continue };
        let (done, ok, outcome) = match app.procs.get(pid) {
            Some(p) if p.is_running() => continue,
            Some(p) => {
                let v = p.view();
                (true, v.status == "exited", p.outcome())
            }
            // The console restarted: the process is gone.
            None => (
                true,
                false,
                "the agent's process is gone (the console restarted)".to_string(),
            ),
        };
        if !done {
            continue;
        }
        let column = if ok { "review" } else { "failed" };
        let result = clip(&outcome, 20_000);
        let _ = app.board.edit(it.id, |x| {
            x.column = column.into();
            x.result = Some(result.clone());
            x.notes.push(Note {
                ts: now(),
                text: format!(
                    "agent process #{} {}",
                    pid,
                    if ok { "finished" } else { "failed" }
                ),
            });
            Ok(())
        });
        if let Some(c) = it
            .source
            .as_ref()
            .and_then(|s| settings.connector(&s.connector))
        {
            let comment = format!(
                "indra agent process #{} {}.\n\n{}",
                pid,
                if ok {
                    "finished; it is ready for review"
                } else {
                    "did not finish"
                },
                clip(&result, 6000)
            );
            report(app, it.id, c, column, &comment).await;
        }
    }
    // 2. Scheduled imports, then 3. automatic runs.
    for c in &settings.connectors {
        if let Some(m) = c.poll_minutes.filter(|m| *m > 0) {
            let last = app
                .board
                .polled
                .lock()
                .unwrap()
                .get(&c.name)
                .copied()
                .unwrap_or(0);
            if now().saturating_sub(last) >= m * 60 {
                app.polled(&c.name);
                if let Err(e) = import(app, &c.name).await {
                    eprintln!("indra: import from {} failed: {}", c.name, e);
                }
            }
        }
        if !c.auto_run {
            continue;
        }
        let node = c
            .node
            .clone()
            .unwrap_or_else(|| crate::deck::nodes::LOCAL.into());
        for it in app.board.items().into_iter().filter(|i| {
            i.column == "todo"
                && i.procs.is_empty()
                && i.source
                    .as_ref()
                    .map(|s| s.connector == c.name)
                    .unwrap_or(false)
        }) {
            if app.procs.running_on(&node) >= settings.capacity(&node) {
                break;
            }
            let req = RunReq {
                profile: None,
                runner: None,
                node: None,
                target: None,
            };
            if let Err(e) = run(app, it.id, req).await {
                app.board
                    .note(it.id, format!("could not start automatically: {}", e));
            }
        }
    }
}

impl App {
    fn polled(&self, connector: &str) {
        self.board
            .polled
            .lock()
            .unwrap()
            .insert(connector.to_string(), now());
    }
}

/// Settings a board view needs alongside the items.
pub fn connectors_view(s: &Settings) -> serde_json::Value {
    serde_json::json!(s
        .connectors
        .iter()
        .map(|c| serde_json::json!({
            "name": c.name, "kind": c.kind, "project": c.project, "auto_run": c.auto_run,
            "poll_minutes": c.poll_minutes, "states": c.states, "profile": c.profile,
        }))
        .collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(key: &str) -> Remote {
        Remote {
            key: key.into(),
            title: format!("item {}", key),
            body: String::new(),
            url: String::new(),
            state: "open".into(),
        }
    }

    #[test]
    fn boards_hold_items_and_trackers_feed_one_each() {
        let dir = std::env::temp_dir().join(format!("indra-boards-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("board.json");
        let b = Board::open(path.clone());
        // Out of the box: one board, and items land on it.
        assert_eq!(b.boards().len(), 1);
        assert_eq!(b.boards()[0].id, MAIN);
        let first = b.create("old item", Patch::default(), None).unwrap();
        assert_eq!(b.get(first).unwrap().board, MAIN);
        // A new board; names are unique, ids come from them.
        let api = b.add_board("API team", "").unwrap();
        assert_eq!(api, "api-team");
        assert!(b.add_board("api TEAM", "").is_err());
        assert_eq!(b.add_board("API team!", "").unwrap(), "api-team-2");
        let on_api = b
            .create(
                "fix auth",
                Patch {
                    board: Some(api.clone()),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert_eq!(b.get(on_api).unwrap().board, api);
        assert!(b
            .create(
                "x",
                Patch {
                    board: Some("nope".into()),
                    ..Default::default()
                },
                None
            )
            .is_err());
        // A tracker feeds the board that has it, and only that one.
        b.update_board(&api, None, None, Some(vec!["jira".into()]))
            .unwrap();
        b.update_board("api-team-2", None, None, Some(vec!["jira".into()]))
            .unwrap();
        assert!(b
            .boards()
            .iter()
            .find(|x| x.id == api)
            .unwrap()
            .connectors
            .is_empty());
        b.merge("jira", vec![remote("J-1")]);
        b.merge("github", vec![remote("G-1")]);
        let on = |key: &str| {
            b.items()
                .into_iter()
                .find(|i| i.source.as_ref().is_some_and(|s| s.key == key))
                .unwrap()
                .board
        };
        assert_eq!(on("J-1"), "api-team-2");
        assert_eq!(on("G-1"), MAIN);
        // Rename "main", move an item, and the data survives a reopen.
        b.update_board(MAIN, Some("Inbox".into()), None, None)
            .unwrap();
        b.update(
            first,
            Patch {
                board: Some(api.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        let b = Board::open(path);
        assert_eq!(b.boards()[0].name, "Inbox");
        assert_eq!(b.get(first).unwrap().board, api);
        // Deleting: never the first board; items move or go with it.
        assert!(b.delete_board(MAIN, None).is_err());
        assert!(b.delete_board(&api, Some(&api)).is_err());
        assert_eq!(b.delete_board(&api, Some(MAIN)).unwrap(), 2);
        assert_eq!(b.get(on_api).unwrap().board, MAIN);
        assert_eq!(b.delete_board("api-team-2", None).unwrap(), 1);
        assert!(b.items().iter().all(|i| i.board == MAIN));
        assert_eq!(b.boards().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
