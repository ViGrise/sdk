use std::io::{BufRead, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use vigrise_cli::login::{login, logout, AssumableRole, LoginOptions, Ui};
use vigrise_cli::profile::{describe_session, read_session};

/// Sign in with your ViGrise account and get role session credentials for the
/// ViGrise SDKs.
#[derive(Parser)]
#[command(name = "vigrise", version)]
struct Cli {
  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand)]
enum Command {
  /// Sign in and write a role session to a profile of ~/.vigrise/credentials.
  Login {
    #[arg(long, env = "VIGRISE_PROFILE", default_value = "default")]
    profile: String,
    /// Role to assume; without it, choose from the roles you may assume.
    #[arg(long)]
    role: Option<String>,
    /// Project you act from; defaults to the role's project.
    #[arg(long)]
    project: Option<String>,
    /// Session length in seconds, 900 up to the role's maximum.
    #[arg(long)]
    duration: Option<u32>,
    /// Sign in by entering a code on another device instead of a local browser.
    #[arg(long)]
    device: bool,
    #[arg(
      long,
      env = "VIGRISE_ACCOUNTS_URL",
      default_value = "https://accounts.vigrise.com/o"
    )]
    accounts_url: String,
    #[arg(
      long,
      env = "VIGRISE_PLATFORM_URL",
      default_value = "https://platform.vigrise.com"
    )]
    platform_url: String,
    #[arg(long, env = "VIGRISE_CLI_CLIENT_ID", default_value = "vigrise-cli")]
    client_id: String,
  },
  /// Remove a profile's session and cached sign-in.
  Logout {
    #[arg(long, env = "VIGRISE_PROFILE", default_value = "default")]
    profile: String,
  },
  /// Show the role and expiry of a profile's session.
  Whoami {
    #[arg(long, env = "VIGRISE_PROFILE", default_value = "default")]
    profile: String,
  },
}

fn vigrise_dir() -> PathBuf {
  let home = std::env::var_os("HOME")
    .or_else(|| std::env::var_os("USERPROFILE"))
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("."));
  home.join(".vigrise")
}

fn credentials_file() -> PathBuf {
  std::env::var_os("VIGRISE_SHARED_CREDENTIALS_FILE")
    .map(PathBuf::from)
    .unwrap_or_else(|| vigrise_dir().join("credentials"))
}

struct Terminal;

impl Ui for Terminal {
  fn open_browser(&self, url: &str) {
    eprintln!("Opening your browser to sign in. If it does not open, visit:\n\n  {url}\n");
    let opener: &[&str] = if cfg!(target_os = "macos") {
      &["open"]
    } else if cfg!(windows) {
      &["cmd", "/C", "start", ""]
    } else {
      &["xdg-open"]
    };
    let _ = std::process::Command::new(opener[0])
      .args(&opener[1..])
      .arg(url)
      .stdout(std::process::Stdio::null())
      .stderr(std::process::Stdio::null())
      .spawn();
  }

  fn show_device_code(&self, user_code: &str, verification_uri: &str, complete: Option<&str>) {
    eprintln!("To sign in, visit {verification_uri} and enter the code:\n\n  {user_code}\n");
    if let Some(complete) = complete {
      eprintln!("Or open {complete}\n");
    }
    eprintln!("Waiting for approval...");
  }

  fn choose_role(&self, roles: &[AssumableRole]) -> Option<usize> {
    if roles.is_empty() {
      eprintln!("You may assume no role in your projects; name one with --role.");
      return None;
    }
    for (index, role) in roles.iter().enumerate() {
      eprintln!("  {}. {} ({})", index + 1, role.name, role.role_vrn);
    }
    eprint!("Role [1-{}]: ", roles.len());
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).ok()?;
    let chosen: usize = line.trim().parse().ok()?;
    (1..=roles.len()).contains(&chosen).then(|| chosen - 1)
  }
}

#[tokio::main]
async fn main() {
  let cli = Cli::parse();
  let result = match cli.command {
    Command::Login {
      profile,
      role,
      project,
      duration,
      device,
      accounts_url,
      platform_url,
      client_id,
    } => {
      let options = LoginOptions {
        accounts_url,
        platform_url,
        client_id,
        profile: profile.clone(),
        role_vrn: role,
        project_id: project,
        duration_seconds: duration,
        device,
        credentials_file: credentials_file(),
        sso_dir: vigrise_dir().join("sso"),
      };
      login(&options, &Terminal).await.map(|session| {
        println!(
          "Profile {profile}: {}",
          describe_session(&session, chrono::Utc::now())
        );
      })
    }
    Command::Logout { profile } => {
      logout(&credentials_file(), &vigrise_dir().join("sso"), &profile)
        .map(|()| println!("Signed out of profile {profile}"))
    }
    Command::Whoami { profile } => match read_session(&credentials_file(), &profile) {
      Ok(Some(session)) => {
        println!("{}", describe_session(&session, chrono::Utc::now()));
        Ok(())
      }
      Ok(None) => {
        eprintln!("Profile {profile} holds no role session; run vigrise login");
        std::process::exit(1);
      }
      Err(error) => Err(error.into()),
    },
  };
  if let Err(error) = result {
    eprintln!("vigrise: {error}");
    std::process::exit(1);
  }
}
