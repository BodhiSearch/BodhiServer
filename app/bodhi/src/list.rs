use crate::hf::{list_models, ModelItem};
use prettytable::{
  format::{self},
  row, Cell, Row, Table,
};
use regex::Regex;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct RemoteModel {
  pub(super) display_name: String,
  pub(super) family: Option<String>,
  pub(super) repo: String,
  pub(super) chat_template: Option<String>,
  pub(super) features: Vec<String>,
  pub(super) files: Vec<String>,
  pub(super) default: String,
}

impl RemoteModel {
  fn variants(&self) -> Vec<String> {
    let re = Regex::new(r".*\.(?P<variant>[^\.]*)\.gguf").unwrap();
    self
      .files
      .iter()
      .map(|f| match re.captures(f) {
        Some(captures) => captures["variant"].to_string(),
        None => f.to_string(),
      })
      .collect::<Vec<String>>()
  }

  fn default(&self) -> String {
    let re = Regex::new(r".*\.(?P<variant>[^\.]*)\.gguf").unwrap();
    let Some(cap) = re.captures(&self.default) else {
      return self.default.to_string();
    };
    cap["variant"].to_string()
  }
}

pub(super) const MODELS_YAML: &str = include_str!("models.yaml");

pub(crate) fn find_remote_model(id: &str) -> Option<RemoteModel> {
  let models: Vec<RemoteModel> = serde_yaml::from_str(MODELS_YAML).ok()?;
  models.into_iter().find(|model| model.display_name.eq(id))
}

pub struct List {
  remote: bool,
}

impl List {
  pub fn new(remote: bool) -> Self {
    Self { remote }
  }

  pub fn execute(self) -> anyhow::Result<()> {
    if self.remote {
      self.list_remote_models()?;
    } else {
      self.list_local_models()?;
    }
    Ok(())
  }

  fn list_local_models(self) -> anyhow::Result<()> {
    let mut table = Table::new();
    table.add_row(row!["NAME", "REPO ID", "SHA", "SIZE", "MODIFIED"]);
    let models = list_models();

    table = models.into_iter().fold(table, |mut table, model| {
      let ModelItem {
        name,
        repo,
        sha,
        size,
        updated,
        ..
      } = model;
      let human_size = size
        .map(|size| format!("{:.2} GB", size as f64 / 1e9))
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
      table.add_row(Row::from(vec![
        Cell::new(&name),
        Cell::new(&repo),
        Cell::new(&sha[..8]),
        Cell::new(&human_size),
        Cell::new(&humantime),
      ]));
      table
    });
    table.set_format(format::FormatBuilder::default().padding(2, 2).build());
    table.printstd();
    Ok(())
  }

  fn list_remote_models(self) -> anyhow::Result<()> {
    let models: Vec<RemoteModel> = serde_yaml::from_str(MODELS_YAML)?;
    let mut table = Table::new();
    table.add_row(row![
      "ID",
      "REPO ID",
      "FAMILY",
      "FEATURES",
      "CHAT TEMPLATE",
      "VARIANTS",
      "DEFAULT"
    ]);
    for model in models.into_iter() {
      let chat_template = model.chat_template.as_deref().unwrap_or("-");
      let chat_template = &truncate(chat_template, 16);
      let variants = model
        .variants()
        .into_iter()
        .fold(vec![String::from("")], |mut fold, item| {
          if fold.last().unwrap().len() > 24 {
            fold.push(String::new());
          }
          let last = fold.last_mut().unwrap();
          if !last.is_empty() {
            last.push_str(", ");
          }
          last.push_str(item.as_str());
          fold
        })
        .join(",\n");
      table.add_row(Row::from(vec![
        Cell::new(&model.display_name),
        Cell::new(&model.repo),
        Cell::new(model.family.as_deref().unwrap_or("")),
        Cell::new(&model.features.join(",")),
        Cell::new(chat_template),
        Cell::new(&variants),
        Cell::new(&model.default()),
      ]));
    }
    table.set_format(format::FormatBuilder::default().padding(2, 2).build());
    table.printstd();
    Ok(())
  }
}

fn truncate(s: &str, max_len: usize) -> String {
  if s.len() <= max_len {
    return s.to_string();
  }

  let half_len = (max_len / 2) - 2;
  let start = &s[0..half_len];
  let end = &s[(s.len() - half_len)..];
  format!("{}...{}", start, end)
}
