# Dump the UI Automation tree of the running rustfm-proto window (what a screen reader sees).
# No elevation needed. Usage: uia-dump.ps1 [-MaxDepth 6] [-MaxNodes 80]
param([int]$MaxDepth = 6, [int]$MaxNodes = 80)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$p = Get-Process rustfm-proto -ErrorAction Stop | Select-Object -First 1
$root = [System.Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)
$walker = [System.Windows.Automation.TreeWalker]::RawViewWalker
$script:n = 0
function Dump($el, $depth) {
    if ($null -eq $el -or $depth -gt $MaxDepth -or $script:n -ge $MaxNodes) { return }
    $script:n++
    $c = $el.Current
    $sel = ""
    try { $sp = $el.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern); if ($sp) { $sel = " selected=" + $sp.Current.IsSelected } } catch {}
    $r = $c.BoundingRectangle
    "{0}{1} | name='{2}' | focusable={3} focused={4}{5} | rect={6},{7} {8}x{9}" -f ("  " * $depth), $c.ControlType.ProgrammaticName, $c.Name, $c.IsKeyboardFocusable, $c.HasKeyboardFocus, $sel, [int]$r.X, [int]$r.Y, [int]$r.Width, [int]$r.Height
    $child = $walker.GetFirstChild($el)
    while ($null -ne $child -and $script:n -lt $MaxNodes) { Dump $child ($depth + 1); $child = $walker.GetNextSibling($child) }
}
"focused element: " + [System.Windows.Automation.AutomationElement]::FocusedElement.Current.Name + " (" + [System.Windows.Automation.AutomationElement]::FocusedElement.Current.ControlType.ProgrammaticName + ")"
Dump $root 0
"nodes printed: $script:n"
