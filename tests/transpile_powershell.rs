use aethershell::transpile::powershell::transpile_powershell_to_ae;

/// Remove all ASCII whitespace to make tests resilient to formatting.
fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_ascii_whitespace()).collect()
}

#[test]
fn simple_assignment() {
    let ps = "$x = 42";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("let x = 42;"), "got:\n{}", ae);
}

#[test]
fn string_assignment() {
    let ps = "$name = \"hello\"";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("let name = \"hello\";"), "got:\n{}", ae);
}

#[test]
fn bool_true_assignment() {
    let ps = "$flag = $true";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("let flag = true;"), "got:\n{}", ae);
}

#[test]
fn bool_false_assignment() {
    let ps = "$flag = $false";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("let flag = false;"), "got:\n{}", ae);
}

#[test]
fn null_assignment() {
    let ps = "$x = $null";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("let x = null;"), "got:\n{}", ae);
}

#[test]
fn array_assignment() {
    let ps = "$items = @(1, 2, 3)";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("let items = ["), "got:\n{}", ae);
}

#[test]
fn cmdlet_get_childitem() {
    let ps = "Get-ChildItem";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("ls"), "got:\n{}", ae);
}

#[test]
fn cmdlet_get_content() {
    let ps = "Get-Content 'file.txt'";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("read_text"), "got:\n{}", ae);
}

#[test]
fn cmdlet_write_host() {
    let ps = "Write-Host 'Hello'";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("echo("), "got:\n{}", ae);
}

#[test]
fn cmdlet_get_process() {
    let ps = "Get-Process";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("proc_list"), "got:\n{}", ae);
}

#[test]
fn preserves_comments() {
    let ps = "# PowerShell comment\nWrite-Host 'hi'";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("// PowerShell comment"), "got:\n{}", ae);
}

#[test]
fn block_comment() {
    let ps = "<# block comment #>";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("// block comment"), "got:\n{}", ae);
}

#[test]
fn if_block_accumulation() {
    let ps = "if ($true) {\n  Write-Host 'yes'\n}";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("sh("), "got:\n{}", ae);
    assert!(ae.contains("if"), "got:\n{}", ae);
}

#[test]
fn foreach_block() {
    let ps = "foreach ($item in $list) {\n  Write-Host $item\n}";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("foreach") && ae.contains("sh("), "got:\n{}", ae);
}

#[test]
fn function_block() {
    let ps = "function Get-Greeting {\n  param($Name)\n  Write-Host \"Hello $Name\"\n}";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("function") && ae.contains("sh("), "got:\n{}", ae);
}

#[test]
fn try_catch_block() {
    let ps = "try {\n  Get-Content 'missing.txt'\n} catch {\n  Write-Host 'error'\n}";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("try") && ae.contains("catch"), "got:\n{}", ae);
}

#[test]
fn switch_block() {
    let ps = "switch ($x) {\n  1 { 'one' }\n  2 { 'two' }\n  default { 'other' }\n}";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("switch"), "got:\n{}", ae);
}

#[test]
fn cmdlet_aliases() {
    let ae_ls = transpile_powershell_to_ae("ls").expect("transpile ok");
    let ae_dir = transpile_powershell_to_ae("dir").expect("transpile ok");
    assert!(ae_ls.contains("ls"), "got:\n{}", ae_ls);
    assert!(ae_dir.contains("ls"), "got:\n{}", ae_dir);
}

#[test]
fn while_block() {
    let ps = "while ($true) {\n  Start-Sleep 1\n}";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    assert!(ae.contains("while") && ae.contains("sh("), "got:\n{}", ae);
}

#[test]
fn transpile_header() {
    let ae = transpile_powershell_to_ae("Write-Host 'hello'").expect("transpile ok");
    assert!(ae.starts_with("// Transpiled from PowerShell"), "got:\n{}", ae);
}

#[test]
fn nested_blocks() {
    let ps = "function Foo {\n  if ($true) {\n    Write-Host 'nested'\n  }\n}";
    let ae = transpile_powershell_to_ae(ps).expect("transpile ok");
    let sh_count = ae.matches("sh(").count();
    assert_eq!(sh_count, 1, "Expected single sh() call for nested blocks, got:\n{}", ae);
}