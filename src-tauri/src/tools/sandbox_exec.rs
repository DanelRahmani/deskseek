//! Where a shell command actually runs.
//!
//! Three rungs, strongest first: a throwaway Docker container, a WSL2 distro, or
//! this machine. The first two are isolation. The third is the protection that
//! already applies everywhere — the workspace path sandbox, the scrubbed child
//! environment, the timeout, the cancel token and the safety guard. Best-effort
//! is not a security boundary, and the UI says so rather than implying otherwise.
//!
//! ponytail: Windows Sandbox, the fourth rung in the plan, is deliberately
//! absent. It boots a whole VM per command, hands nothing back on stdout, and
//! needs a mapped folder to get results out — minutes per command for isolation
//! the two rungs above already give. Add it if someone specifically needs a
//! Windows-guest boundary.

use std::path::Path;

use tokio::process::Command;

use crate::tools::exec;

pub const DEFAULT_IMAGE: &str = "alpine:latest";

/// What the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The strongest rung this machine can actually reach.
    Auto,
    Host,
    Docker,
    Wsl,
}

impl Mode {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "host" => Mode::Host,
            "docker" => Mode::Docker,
            "wsl" => Mode::Wsl,
            _ => Mode::Auto,
        }
    }
}

/// What this machine can actually do.
///
/// `docker_running` — not `docker_cli` — is what gates the container rung. A
/// stopped Docker Desktop answers nothing on its named pipe, and falling
/// through to a container we cannot reach produces a wall of connection
/// errors, which is a worse experience than running the command with a note.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capability {
    pub docker_running: bool,
    pub wsl_distro: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    Docker,
    Wsl,
    Host,
}

impl Rung {
    pub fn as_str(self) -> &'static str {
        match self {
            Rung::Docker => "docker",
            Rung::Wsl => "wsl",
            Rung::Host => "host",
        }
    }
}

/// The decision, resolved once per run so every command in it takes the same
/// path. `note` explains a reply the user asked for and did not get.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub rung: Rung,
    pub image: String,
    pub distro: Option<String>,
    pub note: Option<String>,
}

const DOCKER_DOWN: &str = "Docker was pinned but its engine is not answering — this ran on your machine instead. Start Docker Desktop and re-check to get the container back.";
const WSL_MISSING: &str = "WSL was pinned but no distribution is installed — this ran on your machine instead.";

/// Never fails. A pinned rung whose prerequisite has gone missing falls back to
/// the host with a note, because refusing to run the command at all would be a
/// worse answer than running it and saying why.
pub fn plan_for(mode: Mode, caps: &Capability, image: &str) -> Plan {
    let image = if image.trim().is_empty() {
        DEFAULT_IMAGE.to_string()
    } else {
        image.trim().to_string()
    };
    let host = |note: Option<String>| Plan {
        rung: Rung::Host,
        image: image.clone(),
        distro: None,
        note,
    };

    match mode {
        Mode::Host => host(None),
        Mode::Docker if caps.docker_running => Plan {
            rung: Rung::Docker,
            image,
            distro: None,
            note: None,
        },
        Mode::Docker => host(Some(DOCKER_DOWN.to_string())),
        Mode::Wsl if caps.wsl_distro.is_some() => Plan {
            rung: Rung::Wsl,
            image,
            distro: caps.wsl_distro.clone(),
            note: None,
        },
        Mode::Wsl => host(Some(WSL_MISSING.to_string())),
        Mode::Auto if caps.docker_running => Plan {
            rung: Rung::Docker,
            image,
            distro: None,
            note: None,
        },
        Mode::Auto if caps.wsl_distro.is_some() => Plan {
            rung: Rung::Wsl,
            image,
            distro: caps.wsl_distro.clone(),
            note: None,
        },
        Mode::Auto => host(None),
    }
}

/// The line prepended to every command's output, so where it ran is never a
/// mystery to the model reading the result.
pub fn banner(plan: &Plan) -> String {
    let place = match plan.rung {
        Rung::Docker => format!("docker ({}, no network)", plan.image),
        Rung::Wsl => match &plan.distro {
            Some(name) => format!("wsl ({name})"),
            None => "wsl".to_string(),
        },
        Rung::Host => "this machine".to_string(),
    };
    match &plan.note {
        Some(note) => format!("[ran on {place}] {note}"),
        None => format!("[ran on {place}]"),
    }
}

/// `C:\Users\x` -> `/mnt/c/Users/x`. `None` for anything without a drive
/// letter — a UNC share has no path inside a distro, so the WSL rung is refused
/// rather than failing with something inscrutable.
pub fn wsl_path(path: &Path) -> Option<String> {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut chars = text.chars();
    let drive = chars.next()?;
    if !drive.is_ascii_alphabetic() || chars.next()? != ':' {
        return None;
    }
    let rest = chars.as_str().trim_start_matches('/');
    Some(format!("/mnt/{}/{}", drive.to_ascii_lowercase(), rest))
}

/// The distro names in a `wsl -l -v` listing, minus Docker's own internals.
///
/// Two traps here. The listing is a table, so the header has to be seen before
/// any row is believed: with nothing installed, `wsl.exe` prints a sentence in
/// the OS language, which would otherwise parse as a very odd distro name. And
/// `docker-desktop` is a utility VM holding a filesystem, not somewhere anyone
/// can work, so offering it as a place to run commands would be a lie.
pub fn parse_distros(listing: &str) -> Vec<String> {
    let mut lines = listing.lines();
    if !lines.any(|line| line.trim_start().starts_with("NAME")) {
        return Vec::new();
    }
    lines
        .filter_map(|line| line.trim().trim_start_matches('*').split_whitespace().next())
        .filter(|name| !name.is_empty() && !name.starts_with("docker-desktop"))
        .map(str::to_string)
        .collect()
}

/// Builds the child process for a rung. Only the host rung needs a working
/// directory set on our side; the other two are told where to work in their own
/// arguments.
pub fn command(plan: &Plan, command: &str, ws: &Path, timeout_secs: u64) -> Result<Command, String> {
    match plan.rung {
        Rung::Docker => {
            let mut cmd = Command::new("docker");
            cmd.args([
                "run",
                "--rm",
                "--network",
                "none",
                "--memory",
                "2g",
                "--cpus",
                "2",
            ])
            .arg("-v")
            .arg(format!("{}:/workspace", ws.display()))
            .args(["-w", "/workspace"])
            .arg(&plan.image);
            // `--rm` only removes a container that exits by itself, and killing
            // the docker CLI does not kill the container. Giving the command its
            // own timeout makes it exit at roughly the moment we give up, which
            // is what lets --rm clean up after us.
            cmd.arg("timeout").arg(format!("{timeout_secs}s"));
            cmd.args(["sh", "-lc"]).arg(command);
            Ok(cmd)
        }
        Rung::Wsl => {
            let dir = wsl_path(ws).ok_or_else(|| {
                format!(
                    "'{}' has no drive letter, so WSL cannot reach it — this workspace needs \
                     the host rung",
                    ws.display()
                )
            })?;
            let mut cmd = Command::new("wsl");
            if let Some(distro) = &plan.distro {
                cmd.arg("-d").arg(distro);
            }
            cmd.arg("--cd").arg(dir).args(["-e", "sh", "-lc"]).arg(command);
            Ok(cmd)
        }
        Rung::Host => {
            let mut cmd = if cfg!(windows) {
                Command::new("cmd")
            } else {
                Command::new("sh")
            };
            cmd.arg(if cfg!(windows) { "/C" } else { "-c" }).arg(command);
            Ok(cmd)
        }
    }
}

/// Probes what is available. Bounded and slow-tolerant: this runs in the
/// background at startup and again whenever the setup screen re-checks, so a
/// machine without Docker answers in a second rather than holding anything up.
pub async fn detect() -> Capability {
    let docker_cli = exec::probe("docker", &["--version"], 6).await;
    // Only worth asking the engine if the CLI exists at all.
    let docker_running = docker_cli && exec::probe("docker", &["info"], 15).await;
    Capability {
        docker_running,
        wsl_distro: detect_wsl().await,
    }
}

#[cfg(windows)]
async fn detect_wsl() -> Option<String> {
    let listing = exec::capture("wsl", &["-l", "-v"], 6).await?;
    parse_distros(&listing).into_iter().next()
}

#[cfg(not(windows))]
async fn detect_wsl() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(docker_running: bool, distro: Option<&str>) -> Capability {
        Capability {
            docker_running,
            wsl_distro: distro.map(str::to_string),
        }
    }

    #[test]
    fn auto_takes_the_strongest_rung_this_machine_can_reach() {
        assert_eq!(plan_for(Mode::Auto, &caps(true, Some("Ubuntu")), "x").rung, Rung::Docker);
        assert_eq!(plan_for(Mode::Auto, &caps(false, Some("Ubuntu")), "x").rung, Rung::Wsl);
        assert_eq!(plan_for(Mode::Auto, &caps(false, None), "x").rung, Rung::Host);
    }

    #[test]
    fn a_docker_cli_with_no_engine_is_never_picked() {
        // The bug this rule exists for: the CLI is on PATH, Docker Desktop is
        // installed and stopped, so a "is docker installed" check says yes.
        // Auto must not send commands at an engine that answers nothing.
        let plan = plan_for(Mode::Auto, &caps(false, None), "x");
        assert_eq!(plan.rung, Rung::Host);
        assert!(plan.note.is_none(), "auto falling back is not worth a note");
    }

    #[test]
    fn a_pinned_rung_that_cannot_be_honoured_falls_back_with_a_note() {
        let docker = plan_for(Mode::Docker, &caps(false, None), "x");
        assert_eq!(docker.rung, Rung::Host);
        assert!(docker.note.as_deref().unwrap().contains("engine"));

        let wsl = plan_for(Mode::Wsl, &caps(false, None), "x");
        assert_eq!(wsl.rung, Rung::Host);
        assert!(wsl.note.as_deref().unwrap().contains("distribution"));

        // ...and pins that can be honoured are honoured, even against Auto's taste.
        assert_eq!(plan_for(Mode::Docker, &caps(true, None), "x").rung, Rung::Docker);
        assert_eq!(plan_for(Mode::Wsl, &caps(true, Some("Ubuntu")), "x").rung, Rung::Wsl);
        // Choosing the host on a machine with Docker running is a real choice.
        assert_eq!(plan_for(Mode::Host, &caps(true, Some("Ubuntu")), "x").rung, Rung::Host);
    }

    #[test]
    fn an_empty_image_falls_back_to_the_default() {
        assert_eq!(plan_for(Mode::Host, &caps(false, None), "  ").image, DEFAULT_IMAGE);
        assert_eq!(plan_for(Mode::Host, &caps(false, None), "rust:1").image, "rust:1");
    }

    #[test]
    fn the_banner_says_where_the_command_ran() {
        let plan = plan_for(Mode::Auto, &caps(true, None), "alpine:latest");
        assert_eq!(banner(&plan), "[ran on docker (alpine:latest, no network)]");
        let plan = plan_for(Mode::Auto, &caps(false, Some("Ubuntu")), "x");
        assert_eq!(banner(&plan), "[ran on wsl (Ubuntu)]");
        let plan = plan_for(Mode::Host, &caps(false, None), "x");
        assert_eq!(banner(&plan), "[ran on this machine]");
    }

    #[test]
    fn a_windows_path_maps_into_the_distro() {
        assert_eq!(
            wsl_path(Path::new(r"C:\Users\danel\Coding\Thing")).unwrap(),
            "/mnt/c/Users/danel/Coding/Thing"
        );
        assert_eq!(wsl_path(Path::new("D:/data")).unwrap(), "/mnt/d/data");
        // No drive letter: a UNC share cannot be reached from inside a distro.
        assert!(wsl_path(Path::new(r"\\server\share")).is_none());
        assert!(wsl_path(Path::new("relative/path")).is_none());
    }

    #[test]
    fn distros_are_read_from_the_table_and_dockers_own_are_skipped() {
        let listing = "  NAME              STATE           VERSION\n* Ubuntu            Running         2\n  docker-desktop    Stopped         2\n  Debian            Stopped         1\n";
        assert_eq!(parse_distros(listing), vec!["Ubuntu", "Debian"]);
    }

    #[test]
    fn a_listing_without_a_header_is_not_a_listing() {
        // What wsl.exe prints when nothing is installed — localised, so the
        // only safe reading is "no distros".
        assert!(parse_distros("Windows Subsystem for Linux has no installed distributions.").is_empty());
        assert!(parse_distros("").is_empty());
        // Docker's utility distro is not a place to run anything.
        let only_docker = "  NAME             STATE    VERSION\n* docker-desktop   Stopped  2\n";
        assert!(parse_distros(only_docker).is_empty());
    }

    #[test]
    fn modes_are_parsed_from_the_strings_stored_in_settings() {
        assert_eq!(Mode::parse("auto"), Mode::Auto);
        assert_eq!(Mode::parse("host"), Mode::Host);
        assert_eq!(Mode::parse("docker"), Mode::Docker);
        assert_eq!(Mode::parse("wsl"), Mode::Wsl);
        // Anything unrecognised — including a value from a future version —
        // degrades to the safe, ordinary answer rather than breaking the run.
        assert_eq!(Mode::parse("nonsense"), Mode::Auto);
        assert_eq!(Mode::parse("  Docker "), Mode::Docker);
    }
}
