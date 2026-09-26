/// The program and arguments that open `url` in the default browser on `os`
/// (`std::env::consts::OS`). The URL is always one argument of a program run
/// directly, so characters such as `&` reach the browser unchanged.
pub fn browser_command(os: &str, url: &str) -> (String, Vec<String>) {
  match os {
    "macos" => ("open".into(), vec![url.into()]),
    "windows" => (
      "rundll32".into(),
      vec!["url.dll,FileProtocolHandler".into(), url.into()],
    ),
    _ => ("xdg-open".into(), vec![url.into()]),
  }
}
