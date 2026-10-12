//! Standing up a local SearXNG instance, so `web_search` works.
//!
//! The setup screen already explains the docker command and waits for a URL to be
//! pasted in, which is honest but asks the user to leave the app for a terminal.
//! This is the same sequence done for them: write the instance's config, start the
//! container, wait until it answers, hand back the URL.
//!
//! **The config is ours rather than the image's, and that is the reason this module
//! exists.** A fresh SearXNG answers HTML and refuses `format=json`, so
//! `web_search` fails against a perfectly healthy instance and the reason is buried
//! in a `settings.yml` inside a docker volume — the failure the setup screen warns
//! about, and the one everybody hits first. Writing that file into a folder we own
//! and bind-mounting it means the instance cannot come up in a shape this app is
//! unable to use.
//!
//! Deliberately not bundled, and the reasoning is worth keeping: SearXNG is a
//! Python application, so shipping it would mean a runtime and a per-platform
//! sidecar in a release that already builds three OSes — and it is **AGPL-3.0**,
//! which is a real licensing obligation to carry in exchange for saving one
//! `docker run`. The container is the honest middle: nothing ships with the app,
//! and the image updates with a pull rather than with a release of DeskSeek.
//!
//! ponytail: this starts a container and stops there. It does not supervise it,
//! does not notice if it dies later, and does not offer to remove it. The ceiling
//! is a user whose instance stops without them knowing why; the upgrade is a
//! status check in the setup screen's Re-check, which already has the probe.

use std::path::Path;
use std::time::Duration;

use crate::tools::{browse, exec, sandbox_exec};

/// The container this app owns, named so a second click finds it rather than
/// stacking up a second copy.
pub const CONTAINER: &str = "deskseek-searxng";
/// The port on this machine.
pub const PORT: u16 = 8888;
/// The community image. No tag: `docker pull` is the update path, and pinning a
/// version inside the app would freeze it at whatever shipped that day.
pub const IMAGE: &str = "searxng/searxng";
/// What SearXNG listens on inside the container.
const CONTAINER_PORT: u16 = 8080;
/// Long enough for a first `docker run`, which pulls the image over the network.
const PULL_TIMEOUT: u64 = 600;
/// Everything else docker does here is instant; this is only a runaway guard.
const DOCKER_TIMEOUT: u64 = 60;
/// How long the instance gets to answer once its container is up. A container
/// that is already warm answers immediately; the first run after a pull also has
/// to build its own database.
const READY_TIMEOUT: Duration = Duration::from_secs(45);
/// `docker inspect` prints one of these, and nothing else it can print means the
/// container is there.
const STATES: [&str; 5] = ["created", "running", "paused", "restarting", "exited"];

/// The URL the instance answers on. The same number the container is published
/// on, and `a_published_port_is_the_url` is what keeps those two honest.
pub fn local_url() -> String {
    format!("http://localhost:{PORT}")
}

/// The config the instance runs on.
///
/// `use_default_settings` is the loader's supported way to override a few keys
/// and inherit the rest, so this stays a handful of lines instead of a settings
/// file that has to be recopied whenever the image moves on.
///
/// `json` under `formats` is the one that matters — see the module doc. The
/// secret is here because SearXNG warns loudly on its shipped default; nothing
/// else ever reads it. The limiter is off because it protects a public instance
/// from its visitors, and this one has exactly one visitor, on loopback.
pub fn settings_yaml(secret: &str) -> String {
    format!(
        "\
# Written by DeskSeek. The image's own defaults are inherited, and this file is
# only rewritten if you delete it, so edits you make here are kept.
use_default_settings: true
server:
  secret_key: \"{secret}\"
  limiter: false
search:
  formats:
    - html
    - json
"
    )
}

/// `docker inspect`, for the container's state — which is also how we find out
/// whether it exists at all.
pub fn inspect_args() -> [&'static str; 4] {
    ["inspect", "--format", "{{.State.Status}}", CONTAINER]
}

/// `docker start`, for a container that is already there.
pub fn start_args() -> [&'static str; 2] {
    ["start", CONTAINER]
}

/// `docker run`, detached, with the config bound in from the folder we wrote.
///
/// Published on **loopback only**. `-p 8888:8080` would put an unauthenticated
/// search instance on the local network, which is not what "install it for me"
/// should mean; nothing outside this machine has any business reaching it.
///
/// `unless-stopped` is what makes this a one-time setup: the instance comes back
/// after a reboot without the app having to remember to start it.
pub fn run_args(config_dir: &Path) -> Vec<String> {
    vec![
        "run".into(),
        "-d".into(),
        "--name".into(),
        CONTAINER.into(),
        "--restart".into(),
        "unless-stopped".into(),
        "-p".into(),
        format!("127.0.0.1:{PORT}:{CONTAINER_PORT}"),
        "-v".into(),
        format!("{}:/etc/searxng", config_dir.display()),
        IMAGE.into(),
    ]
}

/// Whether the container exists, from what `docker inspect` printed.
///
/// Tested because it is a guess about someone else's wording: `exec::capture`
/// merges stderr into the text, so "no such container" arrives looking the same
/// as a state would. Matching the *positive* list rather than the error sentence
/// means an unexpected failure reads as "not there" and takes the `docker run`
/// path — where a real error is reported loudly — instead of being mistaken for a
/// container we can start.
pub fn exists(inspect_output: &str) -> bool {
    STATES.contains(&inspect_output.trim())
}

/// Brings the instance up and returns the URL it answers on, or explains what
/// stopped it.
///
/// Saves nothing: whether the setting follows is the caller's decision, and this
/// module owns the container, not the app's configuration.
pub async fn ensure_running(config_dir: &Path, http: &reqwest::Client) -> Result<String, String> {
    // Installed and running are different answers, and this is the one that
    // matters — everything below fails on a named pipe when the engine is asleep,
    // and that message reads like a broken app rather than a switched-off one.
    if !sandbox_exec::detect().await.docker_running {
        return Err(
            "Docker's engine is not answering, so there is nothing to start the container \
             with. Start Docker Desktop, then try again."
                .to_string(),
        );
    }

    write_config_if_missing(config_dir)?;

    let already = exec::capture("docker", &inspect_args(), DOCKER_TIMEOUT)
        .await
        .is_some_and(|text| exists(&text));

    let said = if already {
        exec::capture("docker", &start_args(), DOCKER_TIMEOUT).await
    } else {
        let args = run_args(config_dir);
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        exec::capture("docker", &borrowed, PULL_TIMEOUT).await
    };

    // Only ever used to explain a failure: success is decided by the instance
    // answering, not by docker's exit code. Those are different questions, and a
    // container that started and then died has the second answer wrong.
    let said = said.unwrap_or_default();
    let url = local_url();

    if wait_until_ready(http, &url).await {
        return Ok(url);
    }

    Err(match said.trim() {
        "" => format!(
            "{url} did not answer within {} seconds. `docker logs {CONTAINER}` has what it \
             printed.",
            READY_TIMEOUT.as_secs()
        ),
        text => format!("the instance did not come up. Docker said: {}", condense(text)),
    })
}

/// Waits for the instance to answer, rather than assuming a started container is
/// a working one. `browse::searxng_ready` is the same probe the setup screen
/// trusts, so a green row here means what a green row means after typing a URL in.
async fn wait_until_ready(http: &reqwest::Client, url: &str) -> bool {
    let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
    loop {
        if browse::searxng_ready(http, url).await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
}

/// Writes the config the first time only.
///
/// ponytail: a second click must not undo a tuning the user made in a file the
/// screen called theirs. The ceiling is that a change to the template above only
/// reaches new installs; the upgrade is a version marker in the file and a
/// rewrite when it is behind.
fn write_config_if_missing(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("could not use {}: {e}", dir.display()))?;
    let file = dir.join("settings.yml");
    if file.exists() {
        return Ok(());
    }
    std::fs::write(&file, settings_yaml(&uuid::Uuid::new_v4().simple().to_string()))
        .map_err(|e| format!("could not write {}: {e}", file.display()))
}

/// Docker explains itself in prose, sometimes in paragraphs. A few lines is
/// enough to act on and short enough for a settings panel.
fn condense(text: &str) -> String {
    let joined = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    joined.chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_carries_the_one_key_a_fresh_instance_lacks() {
        let yaml = settings_yaml("s3cret");
        // The image's defaults are inherited, not replaced, so the file cannot go
        // stale as the image gains settings.
        assert!(yaml.contains("use_default_settings: true"));
        assert!(yaml.contains("secret_key: \"s3cret\""));
        assert!(yaml.contains("limiter: false"));

        // `formats` has to be a *list* nested under `search`. Getting the nesting
        // wrong is accepted by the loader and silently ignored by the API, which
        // is the failure this whole file exists to prevent.
        let search = yaml.split("search:").nth(1).expect("a search: block");
        assert!(search.contains("- json"), "without json, web_search 403s");
        assert!(search.contains("- html"));
    }

    #[test]
    fn the_secret_is_not_the_shipped_one() {
        let one = settings_yaml(&uuid::Uuid::new_v4().simple().to_string());
        let two = settings_yaml(&uuid::Uuid::new_v4().simple().to_string());
        assert_ne!(one, two);
        // The placeholder the image ships with is what makes SearXNG warn on
        // every request; it must never be what we write.
        assert!(!one.contains("ultrasecretkey"));
    }

    #[test]
    fn the_container_is_named_bound_and_published_on_loopback() {
        let args = run_args(Path::new("/data/searxng"));
        assert!(args.contains(&CONTAINER.to_string()), "a named container is one a second click finds");
        assert!(args.contains(&"/data/searxng:/etc/searxng".to_string()), "our config is what it runs on");
        assert!(args.contains(&"-d".to_string()), "detached, or the command never returns");
        assert_eq!(args.last().map(String::as_str), Some(IMAGE));

        let published = args.iter().find(|a| a.contains(":8080")).expect("a published port");
        assert!(
            published.starts_with("127.0.0.1:"),
            "an unauthenticated search instance must not be on the local network, got {published}"
        );
    }

    #[test]
    fn a_published_port_is_the_url() {
        // Two tables that have to agree, in one place: the port the container is
        // published on and the port the app then talks to.
        let args = run_args(Path::new("/x"));
        assert!(args.contains(&format!("127.0.0.1:{PORT}:{CONTAINER_PORT}")));
        assert_eq!(local_url(), format!("http://localhost:{PORT}"));
    }

    #[test]
    fn a_missing_container_is_not_mistaken_for_a_running_one() {
        assert!(exists("running"));
        assert!(exists("exited\n"));
        assert!(exists("created"));
        // Everything docker says when it has not got a container is a sentence,
        // and a sentence is not a state.
        assert!(!exists("Error: No such container: deskseek-searxng"));
        assert!(!exists("Cannot connect to the Docker daemon at npipe:////./pipe/docker_engine."));
        assert!(!exists(""));
    }

    #[test]
    fn a_failure_message_is_short_enough_to_show() {
        // Four non-empty lines, one more than the cap, so "the first few" is a
        // claim the assertion can actually fail on.
        let wall = "Cannot connect to the Docker daemon at npipe:////./pipe/docker_engine.\n\
                    Is the docker daemon running?\n\
                    \n\
                    Start Docker Desktop and wait for it to answer.\n\
                    And a fourth line nobody is going to read.";
        let short = condense(wall);
        assert!(short.contains("Is the docker daemon running?"));
        assert!(
            !short.contains("nobody is going to read"),
            "the first few lines are the useful ones"
        );

        // And the length cap, on input long enough to need it.
        assert_eq!(condense(&"x".repeat(500)).chars().count(), 300);
    }
}
