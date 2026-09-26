use vigrise_cli::opener::browser_command;

#[test]
fn urls_reach_the_opener_as_one_argument_without_a_shell() {
  let url = "https://accounts.test/o/oauth2/auth?response_type=code&client_id=vigrise-cli&x=a|b";
  for os in ["macos", "linux", "windows"] {
    let (program, args) = browser_command(os, url);
    assert_ne!(program, "cmd", "{os}");
    assert_eq!(args.last().map(String::as_str), Some(url), "{os}");
  }
  assert_eq!(browser_command("windows", url).0, "rundll32");
}
