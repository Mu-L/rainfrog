use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Display, Deserialize)]
pub enum MenuPreview {
  Rows,
  Columns,
  Constraints,
  Indexes,
  Policies,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Display, Deserialize)]
pub enum ExportFormat {
  CSV,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Display, Deserialize)]
pub enum Action {
  Tick,
  Render,
  Resize(u16, u16),
  Resume,
  Quit,
  Refresh,
  Error(String),
  Help,
  SubmitEditorQuery,
  SubmitEditorQueryBypassParser,
  Query(Vec<String>, bool, bool), // (query_lines, execution_confirmed, bypass_parser)
  MenuPreview(MenuPreview, String, String), // (preview, schema, table)
  QueryToEditor(Vec<String>),
  ClearHistory,
  AbortQuery,
  FocusMenu,
  FocusEditor,
  FocusHistory,
  FocusData,
  FocusFavorites,
  CycleFocusForwards,
  CycleFocusBackwards,
  LoadMenu,
  CopyData(String),
  RequestExportData(i64),
  ExportData(ExportFormat),
  ExportDataFinished,
  RequestSaveFavorite(Vec<String>),
  SaveFavorite(String, Vec<String>),
  DeleteFavorite(String),
}
