use serde::Deserialize;

/// A page of a list.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Page<T> {
  /// The items.
  pub data: Vec<T>,
  /// Pass to the next call to read the next page; `None` on the last page.
  #[serde(default)]
  pub next_cursor: Option<String>,
}
