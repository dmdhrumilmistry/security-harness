//! `indra completions`: Tab completion for indra's commands and options.
//!
//! `indra completions zsh` prints the script; `--install` sets it up for your
//! shell (the install scripts do this): the script goes under indra's data
//! directory (fish: its own completions folder), and for bash, zsh and
//! PowerShell one marked block in the shell's startup file loads it.
//! `indra update` rewrites installed scripts from the new binary.

use clap::{Args, CommandFactory, ValueEnum};
use clap_complete::Shell;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Args, Debug)]
pub struct CompletionsArgs {
    /// bash, zsh, fish, powershell or elvish (default: your shell)
    shell: Option<Shell>,
    /// Set completion up for that shell (open a new terminal afterwards)
    #[arg(long, conflicts_with = "uninstall")]
    install: bool,
    /// Undo --install
    #[arg(long)]
    uninstall: bool,
    /// Rewrite installed scripts (indra update does this)
    #[arg(long, hide = true)]
    refresh: bool,
    /// Say nothing on success
    #[arg(long, hide = true)]
    quiet: bool,
}

const BEGIN: &str = "# >>> indra completions >>>";
const END: &str = "# <<< indra completions <<<";

/// The shell this user runs: `$SHELL`, else PowerShell on Windows.
fn detect() -> Option<Shell> {
    if let Some(s) = std::env::var_os("SHELL") {
        let name = Path::new(&s).file_name()?.to_string_lossy().to_string();
        return Shell::from_str(name.trim_end_matches(".exe"), true).ok();
    }
    if cfg!(windows) {
        return Some(Shell::PowerShell);
    }
    None
}

pub fn script(shell: Shell) -> Vec<u8> {
    let mut out = Vec::new();
    clap_complete::generate(shell, &mut crate::Cli::command(), "indra", &mut out);
    out
}

fn dir() -> PathBuf {
    crate::paths::data_dir().join("completions")
}

/// Where the script for `shell` goes, and the startup file that loads it
/// (none for fish, which loads its completions folder by itself).
fn places(shell: Shell) -> Option<(PathBuf, Option<PathBuf>)> {
    let home = crate::paths::home()?;
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    Some(match shell {
        Shell::Bash => (dir().join("indra.bash"), Some(home.join(".bashrc"))),
        Shell::Zsh => (
            dir().join("_indra"),
            Some(var("ZDOTDIR").unwrap_or(home).join(".zshrc")),
        ),
        Shell::Fish => (
            var("XDG_CONFIG_HOME")
                .unwrap_or_else(|| home.join(".config"))
                .join("fish/completions/indra.fish"),
            None,
        ),
        Shell::PowerShell => (
            dir().join("indra.ps1"),
            // The install script passes PowerShell's own $PROFILE.
            Some(var("INDRA_PS_PROFILE").unwrap_or_else(|| {
                if cfg!(windows) {
                    home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1")
                } else {
                    home.join(".config/powershell/Microsoft.PowerShell_profile.ps1")
                }
            })),
        ),
        _ => return None,
    })
}

/// The lines the startup file gets (between the markers).
fn block(shell: Shell, script: &Path) -> String {
    let p = script.display().to_string();
    let body = match shell {
        Shell::Zsh => format!(
            "if [ -f \"{p}\" ]; then\n  (( $+functions[compdef] )) || {{ autoload -Uz compinit && compinit -i; }}\n  source \"{p}\"\nfi"
        ),
        Shell::PowerShell => format!("if (Test-Path \"{p}\") {{ . \"{p}\" }}"),
        _ => format!("[ -f \"{p}\" ] && . \"{p}\""),
    };
    format!("{BEGIN}\n{body}\n{END}\n")
}

/// `text` without the marked block (and the blank line before it).
fn strip(text: &str) -> String {
    let (Some(b), Some(e)) = (text.find(BEGIN), text.find(END)) else {
        return text.to_string();
    };
    if e < b {
        return text.to_string();
    }
    let head = text[..b].trim_end_matches('\n');
    let tail = text[e + END.len()..].trim_start_matches('\n');
    let mut out = head.to_string();
    if !out.is_empty() {
        out.push('\n');
    }
    if !tail.is_empty() {
        out.push('\n');
        out.push_str(tail);
    }
    out
}

/// `text` with the block added at the end, or replaced where it is.
fn with_block(text: &str, block: &str) -> String {
    let rest = strip(text);
    let mut out = rest.trim_end_matches('\n').to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(block);
    out
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {}", d.display(), e))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {}", path.display(), e))
}

fn install(shell: Shell, quiet: bool) -> Result<(), String> {
    let (file, rc) = places(shell).ok_or_else(|| {
        format!(
            "--install does not know {} yet: add `indra completions {}` to its startup by hand",
            shell, shell
        )
    })?;
    write(&file, &script(shell))?;
    if let Some(rc) = &rc {
        let old = std::fs::read_to_string(rc).unwrap_or_default();
        let new = with_block(&old, &block(shell, &file));
        if new != old {
            write(rc, new.as_bytes())?;
        }
    }
    if !quiet {
        println!(
            "Tab completion for {} is set up: open a new terminal.",
            shell
        );
        println!("  script  {}", file.display());
        if let Some(rc) = rc {
            println!("  loaded by a marked block in {}", rc.display());
        }
        println!("Undo: indra completions {} --uninstall", shell);
    }
    Ok(())
}

fn uninstall(shell: Shell, quiet: bool) -> Result<(), String> {
    let (file, rc) = places(shell).ok_or("nothing to undo for that shell")?;
    let _ = std::fs::remove_file(&file);
    if let Some(rc) = &rc {
        if let Ok(old) = std::fs::read_to_string(rc) {
            let new = strip(&old);
            if new != old {
                write(rc, new.as_bytes())?;
            }
        }
    }
    if !quiet {
        println!("Tab completion for {} removed.", shell);
    }
    Ok(())
}

/// Rewrite the scripts that are installed (after an update).
fn refresh() {
    for sh in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell] {
        if let Some((file, _)) = places(sh) {
            if file.exists() {
                let _ = write(&file, &script(sh));
            }
        }
    }
}

pub fn run(a: CompletionsArgs) -> Result<ExitCode, String> {
    if a.refresh {
        refresh();
        return Ok(ExitCode::SUCCESS);
    }
    let shell = a
        .shell
        .or_else(detect)
        .ok_or("which shell? e.g. `indra completions zsh` (bash, zsh, fish, powershell, elvish)")?;
    if a.install {
        install(shell, a.quiet)?;
    } else if a.uninstall {
        uninstall(shell, a.quiet)?;
    } else {
        use std::io::Write;
        std::io::stdout()
            .write_all(&script(shell))
            .map_err(|e| e.to_string())?;
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_cover_the_commands() {
        let zsh = String::from_utf8(script(Shell::Zsh)).unwrap();
        for c in ["serve", "update", "completions", "user", "claude"] {
            assert!(zsh.contains(c), "{} missing", c);
        }
        assert!(zsh.contains("compdef _indra indra"));
        let bash = String::from_utf8(script(Shell::Bash)).unwrap();
        assert!(bash.contains("complete -F _indra"));
        assert!(!String::from_utf8(script(Shell::Fish)).unwrap().is_empty());
        assert!(!String::from_utf8(script(Shell::PowerShell))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn the_startup_block_is_added_once_replaced_and_removed() {
        let rc = "export A=1\n\nalias x=y\n";
        let b1 = block(Shell::Bash, Path::new("/d/indra.bash"));
        let once = with_block(rc, &b1);
        assert!(once.starts_with("export A=1\n\nalias x=y\n\n# >>> indra"));
        assert_eq!(with_block(&once, &b1), once, "idempotent");
        let b2 = block(Shell::Bash, Path::new("/other/indra.bash"));
        let moved = with_block(&once, &b2);
        assert!(moved.contains("/other/") && !moved.contains("/d/"));
        assert_eq!(moved.matches(BEGIN).count(), 1);
        assert_eq!(strip(&moved), rc.trim_end_matches('\n').to_string() + "\n");
        // Code after the block survives its removal.
        let mid = format!("a\n\n{}\nb\n", b1.trim_end());
        assert_eq!(strip(&mid), "a\n\nb\n");
        assert_eq!(with_block("", &b1), b1);
        // A path with a space (macOS: Application Support) stays one word.
        let z = block(
            Shell::Zsh,
            Path::new("/Users/me/Library/Application Support/indra/completions/_indra"),
        );
        assert!(
            z.contains("source \"/Users/me/Library/Application Support/indra/completions/_indra\"")
        );
    }
}
