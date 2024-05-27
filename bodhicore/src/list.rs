use crate::hf::{list_models, LocalModel};
use derive_new::new;
use prettytable::{
  format::{self},
  row, Cell, Row, Table,
};
use serde::Deserialize;

pub(super) const MODELS_YAML: &str = include_str!("models.yaml");

#[allow(clippy::too_many_arguments)]
#[derive(Debug, Deserialize, Default, PartialEq, Clone, new)]
pub(super) struct RemoteModel {
  pub(super) alias: String,
  pub(super) family: String,
  pub(super) repo: String,
  pub(super) filename: String,
  pub(super) features: Vec<String>,
  pub(super) chat_template: String,
}

impl From<RemoteModel> for Row {
  fn from(model: RemoteModel) -> Self {
    Row::from(vec![
      &model.alias,
      &model.family,
      &model.repo,
      &model.filename,
      &model.features.join(","),
      &model.chat_template,
    ])
  }
}

impl From<LocalModel> for Row {
  fn from(model: LocalModel) -> Self {
    let LocalModel {
      name,
      repo,
      sha,
      size,
      updated,
      ..
    } = model;
    let human_size = size
      .map(|size| format!("{:.2} GB", size as f64 / 2_f64.powf(30.0)))
      .unwrap_or_else(|| String::from("Unknown"));
    let humantime = || -> Option<String> {
      let updated = updated?;
      let duration = chrono::Utc::now()
        .signed_duration_since(updated)
        .to_std()
        .ok()?;
      let formatted = humantime::format_duration(duration)
        .to_string()
        .split(' ')
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
      Some(formatted)
    }();
    let humantime = humantime.unwrap_or_else(|| String::from("Unknown"));
    Row::from(vec![
      Cell::new(&name),
      Cell::new(&repo),
      Cell::new(&sha[..8]),
      Cell::new(&human_size),
      Cell::new(&humantime),
    ])
  }
}

pub(crate) fn find_remote_model(id: &str) -> Option<RemoteModel> {
  let models: Vec<RemoteModel> = serde_yaml::from_str(MODELS_YAML).ok()?;
  _find_remote_model(models, id)
}

fn _find_remote_model(models: Vec<RemoteModel>, id: &str) -> Option<RemoteModel> {
  models.into_iter().find(|model| model.alias.eq(id))
}

pub enum List {
  Local,
  Remote,
  Models,
}

impl List {
  pub fn new(remote: bool, models: bool) -> Self {
    match (remote, models) {
      (true, false) => List::Remote,
      (false, true) => List::Models,
      (false, false) => List::Local,
      (true, true) => unreachable!("both remote and models cannot be true"),
    }
  }

  pub fn execute(self) -> anyhow::Result<()> {
    match self {
      List::Local => self.list_local_model_alias()?,
      List::Remote => self.list_remote_models()?,
      List::Models => self.list_local_models()?,
    }
    Ok(())
  }

  fn list_local_model_alias(self) -> anyhow::Result<()> {
    todo!()
  }

  fn list_local_models(self) -> anyhow::Result<()> {
    let mut table = Table::new();
    table.add_row(row!["NAME", "REPO ID", "SHA", "SIZE", "MODIFIED"]);
    let models = list_models();
    for row in models.into_iter().map(Row::from) {
      table.add_row(row);
    }
    table.set_format(format::FormatBuilder::default().padding(2, 2).build());
    table.printstd();
    Ok(())
  }

  fn list_remote_models(self) -> anyhow::Result<()> {
    let models: Vec<RemoteModel> = serde_yaml::from_str(MODELS_YAML)?;
    let mut table = Table::new();
    table.add_row(row![
      "ALIAS",
      "FAMILY",
      "REPO",
      "FILENAME",
      "FEATURES",
      "CHAT TEMPLATE"
    ]);
    for row in models.into_iter().map(Row::from) {
      table.add_row(row);
    }
    table.set_format(format::FormatBuilder::default().padding(2, 2).build());
    table.printstd();
    println!();
    println!("To download and configure the model alias, run `bodhi pull <ALIAS>`");
    Ok(())
  }
}

#[cfg(test)]
mod test {
  use super::RemoteModel;
  use crate::{hf::LocalModel, list::_find_remote_model, test_utils::TEST_MODELS_YAML, List};
  use chrono::Utc;
  use prettytable::{Cell, Row};

  #[test]
  fn test_list_find_remote_model_by_id() -> anyhow::Result<()> {
    let llama3_instruct = RemoteModel {
      alias: "llama3:instruct".to_string(),
      ..Default::default()
    };
    let llama2_instruct = RemoteModel {
      alias: "llama2:instruct".to_string(),
      ..Default::default()
    };
    let models = vec![llama3_instruct, llama2_instruct.clone()];
    let model = _find_remote_model(models, "llama2:instruct").unwrap();
    assert_eq!(llama2_instruct, model);
    Ok(())
  }

  #[test]
  fn test_list_remote_model_to_row() -> anyhow::Result<()> {
    let model = serde_yaml::from_str::<Vec<RemoteModel>>(TEST_MODELS_YAML)?
      .first()
      .unwrap()
      .to_owned();
    let row: Row = model.into();
    let expected = Row::from(vec![
      Cell::new("llama3:instruct"),
      Cell::new("llama3"),
      Cell::new("QuantFactory/Meta-Llama-3-8B-Instruct-GGUF"),
      Cell::new("Meta-Llama-3-8B-Instruct.Q8_0.gguf"),
      Cell::new("chat"),
      Cell::new("llama3"),
    ]);
    assert_eq!(expected, row);
    Ok(())
  }

  #[test]
  fn test_list_model_item_to_row() -> anyhow::Result<()> {
    let three_days_back = Utc::now() - chrono::Duration::days(3) - chrono::Duration::hours(1);
    let model = LocalModel {
      name: "Meta-Llama-3-8B-Instruct-GGUF".to_string(),
      repo: "QuantFactory".to_string(),
      path: "models--QuantFactory--Meta-Llama-3-8B-Instruct-GGUF".to_string(),
      sha: "1234567890".to_string(),
      size: Some(1024 * 1024 * 1024 * 10),
      updated: Some(three_days_back),
    };
    let row = model.into();
    let expected = Row::from(vec![
      Cell::new("Meta-Llama-3-8B-Instruct-GGUF"),
      Cell::new("QuantFactory"),
      Cell::new("12345678"),
      Cell::new("10.00 GB"),
      Cell::new("3days 1h"),
    ]);
    assert_eq!(expected, row);
    Ok(())
  }

  #[test]
  fn test_list_local_model_alias() -> anyhow::Result<()> {
    List::new(false, false).execute()?;
    Ok(())
  }
}
