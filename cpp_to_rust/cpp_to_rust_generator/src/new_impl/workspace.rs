use new_impl::database::Database;
use common::errors::Result;
use common::file_utils::PathBufWithAdded;
use common::file_utils::{save_json, load_json, remove_dir_all, create_dir_all, create_dir};
use common::string_utils::CaseOperations;
use common::log;
use config::Config;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

#[derive(Debug, Default)]
#[derive(Serialize, Deserialize)]
pub struct WorkspaceConfig {
  disable_logging: bool,
  write_dependencies_local_paths: bool,
}

#[derive(Debug)]
struct EditedDatabase {
  database: Database,
  saved: bool,
}

#[derive(Debug)]
pub struct Workspace {
  path: PathBuf,
  config: WorkspaceConfig,
  databases: Vec<EditedDatabase>,
}

fn config_path(path: &Path) -> PathBuf {
  path.with_added("config.json")
}

fn database_path(workspace_path: &Path, crate_name: &str) -> PathBuf {
  workspace_path.with_added(crate_name).with_added(
    "database.json",
  )
}


impl Workspace {
  pub fn new(path: PathBuf) -> Result<Workspace> {
    if !path.is_dir() {
      return Err(format!("No such directory: {}", path.display()).into());
    }
    let config_path = config_path(&path);
    let w = Workspace {
      path,
      config: if config_path.exists() {
        load_json(config_path)?
      } else {
        WorkspaceConfig::default()
      },
      databases: Vec::new(),
    };
    w.apply_logger_settings()?;
    Ok(w)
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn tmp_path(&self) -> Result<PathBuf> {
    let path = self.path.with_added("tmp");
    if !path.exists() {
      create_dir(&path)?;
    }
    Ok(path)
  }

  pub fn import_published_crate(&mut self, crate_name: &str) -> Result<()> {
    unimplemented!()
  }

  fn take_loaded_crate(&mut self, crate_name: &str) -> Option<Database> {
    self
      .databases
      .iter()
      .position(|d| d.database.crate_name() == crate_name)
      .and_then(|i| Some(self.databases.swap_remove(i).database))
  }


  pub fn load_crate(&mut self, crate_name: &str) -> Result<Database> {
    if let Some(r) = self.take_loaded_crate(crate_name) {
      return Ok(r);
    }
    load_json(database_path(&self.path, crate_name))
  }

  pub fn load_or_create_crate(&mut self, crate_name: &str) -> Result<Database> {
    if let Some(r) = self.take_loaded_crate(crate_name) {
      return Ok(r);
    }
    let path = database_path(&self.path, crate_name);
    if path.exists() {
      load_json(path)
    } else {
      Ok(Database::empty(crate_name))
    }
  }

  pub fn put_crate(&mut self, database: Database, saved: bool) {
    self.databases.push(EditedDatabase { database, saved });
  }

  pub fn set_disable_logging(&mut self, value: bool) -> Result<()> {
    if self.config.disable_logging == value {
      return Ok(());
    }
    self.config.disable_logging = value;
    self.apply_logger_settings()?;
    self.save_config()
  }

  fn save_config(&self) -> Result<()> {
    save_json(config_path(&self.path), &self.config)
  }

  fn apply_logger_settings(&self) -> Result<()> {
    let mut logger = log::default_logger();
    logger.set_default_settings(log::LoggerSettings {
      file_path: None,
      write_to_stderr: false,
    });
    let mut category_settings = HashMap::new();
    let mut debug_categories = vec![
      log::DebugGeneral,
      log::DebugMoveFiles,
      log::DebugTemplateInstantiation,
      log::DebugInheritance,
      log::DebugParserSkips,
      log::DebugParser,
      log::DebugFfiSkips,
      log::DebugSignals,
      log::DebugAllocationPlace,
      log::DebugRustSkips,
      log::DebugQtDoc,
      log::DebugQtHeaderNames,
    ];
    for category in &[log::Status, log::Error] {
      category_settings.insert(
        *category,
        log::LoggerSettings {
          file_path: None,
          write_to_stderr: true,
        },
      );
    }
    if !self.config.disable_logging {
      let logs_dir = self.path.with_added("log");
      logger.log(
        log::Status,
        format!("Debug log will be saved to {}", logs_dir.display()),
      );
      if logs_dir.exists() {
        remove_dir_all(&logs_dir)?;
      }
      create_dir_all(&logs_dir)?;
      for category in debug_categories {
        let name = format!("{:?}", category).to_snake_case();
        let path = logs_dir.with_added(format!("{}.log", name));
        category_settings.insert(
          category,
          log::LoggerSettings {
            file_path: Some(path),
            write_to_stderr: false,
          },
        );
      }
    }
    logger.set_all_category_settings(category_settings);
    Ok(())
  }

  pub fn process_crate(&mut self, config: &Config) -> Result<()> {
    ::new_impl::processor::process(self, config)
  }

  pub fn save_data(&mut self) -> Result<()> {
    for database in &mut self.databases {
      if !database.saved {
        let data = &database.database;
        save_json(database_path(&self.path, data.crate_name()), &data)?;
        database.saved = true;
      }
    }
    Ok(())
  }
}

impl Drop for Workspace {
  fn drop(&mut self) {
    if let Err(err) = self.save_data() {
      err.display_report();
      ::std::process::exit(1);
    }
  }
}