use std::path::PathBuf;

use color_eyre::eyre::Result;
use directories::{ProjectDirs, UserDirs};
use lazy_static::lazy_static;
use tracing::error;
use tracing_error::ErrorLayer;
use tracing_subscriber::{self, Layer, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

const VERSION_MESSAGE: &str =
  concat!(env!("CARGO_PKG_VERSION"), "-", env!("VERGEN_GIT_DESCRIBE"), " (", env!("VERGEN_BUILD_DATE"), ")");

lazy_static! {
  pub static ref PROJECT_NAME: String = env!("CARGO_CRATE_NAME").to_uppercase().to_string();
  pub static ref DATA_FOLDER: Option<PathBuf> =
    std::env::var(format!("{}_DATA", PROJECT_NAME.clone())).ok().map(PathBuf::from);
  pub static ref CONFIG_FOLDER: Option<PathBuf> =
    std::env::var(format!("{}_CONFIG", PROJECT_NAME.clone())).ok().map(PathBuf::from);
  pub static ref EXPORT_FOLDER: Option<PathBuf> =
    std::env::var(format!("{}_EXPORT", PROJECT_NAME.clone())).ok().map(PathBuf::from);
  pub static ref FAVORITES_FOLDER: Option<PathBuf> =
    std::env::var(format!("{}_FAVORITES", PROJECT_NAME.clone())).ok().map(PathBuf::from);
  pub static ref LOG_ENV: String = format!("{}_LOGLEVEL", PROJECT_NAME.clone());
  pub static ref LOG_FILE: String = format!("{}.log", env!("CARGO_PKG_NAME"));
}

fn project_directory() -> Option<ProjectDirs> {
  ProjectDirs::from("dev", "rainfrog", env!("CARGO_PKG_NAME"))
}

fn user_directory() -> Option<UserDirs> {
  UserDirs::new()
}

pub fn initialize_panic_handler() -> Result<()> {
  let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default()
    .panic_section(format!("This is a bug. Consider reporting it at {}", env!("CARGO_PKG_REPOSITORY")))
    .capture_span_trace_by_default(false)
    .display_location_section(false)
    .display_env_section(false)
    .into_hooks();
  eyre_hook.install()?;
  std::panic::set_hook(Box::new(move |panic_info| {
    if let Ok(mut t) = crate::tui::Tui::new() {
      if let Err(r) = t.exit() {
        error!("Unable to exit Terminal: {:?}", r);
      }
    }

    #[cfg(not(debug_assertions))]
    {
      use human_panic::{Metadata, handle_dump, print_msg};
      let meta = Metadata::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
        .authors(env!("CARGO_PKG_AUTHORS").replace(':', ", "))
        .homepage(env!("CARGO_PKG_HOMEPAGE"));

      let file_path = handle_dump(&meta, panic_info);
      // prints human-panic message
      print_msg(file_path, &meta).expect("human-panic: printing error message to console failed");
      eprintln!("{}", panic_hook.panic_report(panic_info)); // prints color-eyre stack trace to stderr
    }
    let msg = format!("{}", panic_hook.panic_report(panic_info));
    log::error!("Error: {}", strip_ansi_escapes::strip_str(msg));

    #[cfg(debug_assertions)]
    {
      // Better Panic stacktrace that is only enabled when debugging.
      better_panic::Settings::auto()
        .most_recent_first(false)
        .lineno_suffix(true)
        .verbosity(better_panic::Verbosity::Full)
        .create_panic_handler()(panic_info);
    }

    std::process::exit(libc::EXIT_FAILURE);
  }));
  Ok(())
}

pub fn get_favorites_dir() -> PathBuf {
  if let Some(s) = FAVORITES_FOLDER.clone() {
    s
  } else if let Some(proj_dirs) = project_directory() {
    proj_dirs.data_local_dir().to_path_buf()
  } else {
    PathBuf::from(".").join(".favorites")
  }
}

pub fn get_data_dir() -> PathBuf {
  if let Some(s) = DATA_FOLDER.clone() {
    s
  } else if let Some(proj_dirs) = project_directory() {
    proj_dirs.data_local_dir().to_path_buf()
  } else {
    PathBuf::from(".").join(".data")
  }
}

pub fn get_config_dir() -> PathBuf {
  if let Some(s) = CONFIG_FOLDER.clone() {
    s
  } else if let Some(proj_dirs) = project_directory() {
    proj_dirs.config_local_dir().to_path_buf()
  } else {
    PathBuf::from(".").join(".config")
  }
}

pub fn get_export_dir() -> PathBuf {
  if let Some(s) = EXPORT_FOLDER.clone() {
    s
  } else if let Some(user_dir) = user_directory() {
    if let Some(download_dir) = user_dir.download_dir() {
      download_dir.to_path_buf()
    } else {
      PathBuf::from(".").join(".export")
    }
  } else {
    PathBuf::from(".").join(".export")
  }
}

pub fn initialize_logging() -> Result<()> {
  let directory = get_data_dir();
  std::fs::create_dir_all(directory.clone())?;
  let log_path = directory.join(LOG_FILE.clone());
  let log_file = std::fs::File::create(log_path)?;
  // TODO: Audit that the environment access only happens in single-threaded code.
  unsafe {
    std::env::set_var(
      "RUST_LOG",
      std::env::var("RUST_LOG")
        .or_else(|_| std::env::var(LOG_ENV.clone()))
        .unwrap_or_else(|_| format!("{}=info", env!("CARGO_CRATE_NAME"))),
    )
  };
  let file_subscriber = tracing_subscriber::fmt::layer()
    .with_file(true)
    .with_line_number(true)
    .with_writer(log_file)
    .with_target(false)
    .with_ansi(false)
    .with_filter(tracing_subscriber::filter::EnvFilter::from_default_env());
  tracing_subscriber::registry().with(file_subscriber).with(ErrorLayer::default()).init();
  Ok(())
}

/// Similar to the `std::dbg!` macro, but generates `tracing` events rather
/// than printing to stdout.
///
/// By default, the verbosity level for the generated events is `DEBUG`, but
/// this can be customized.
#[macro_export]
macro_rules! trace_dbg {
  (target: $target:expr_2021, level: $level:expr_2021, $ex:expr_2021) => {{
    match $ex {
      value => {
        tracing::event!(target: $target, $level, ?value, stringify!($ex));
        value
      },
    }
  }};
  (level: $level:expr_2021, $ex:expr_2021) => {
    trace_dbg!(target: module_path!(), level: $level, $ex)
  };
  (target: $target:expr_2021, $ex:expr_2021) => {
    trace_dbg!(target: $target, level: tracing::Level::DEBUG, $ex)
  };
  ($ex:expr_2021) => {
    trace_dbg!(level: tracing::Level::DEBUG, $ex)
  };
}

pub fn version() -> String {
  let author = clap::crate_authors!();

  let config_dir_path = get_config_dir().display().to_string();
  let export_dir_path = get_export_dir().display().to_string();
  let data_dir_path = get_data_dir().display().to_string();
  let favorites_dir_path = get_favorites_dir().display().to_string();

  format!(
    "\
{VERSION_MESSAGE}

Authors: {author}

Config directory: {config_dir_path}
Export directory: {export_dir_path}
Data directory: {data_dir_path}
Favorites directory: {favorites_dir_path}"
  )
}
