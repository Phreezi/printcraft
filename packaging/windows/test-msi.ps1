<#
.SYNOPSIS
  Check the compiled MSI's install scope, publisher, PeDeeFe identity, shortcuts, PDF verbs (open, print, printto), PDF context menu, native UI and OCR models, without installing it.
.EXAMPLE
  pwsh packaging/windows/test-msi.ps1 dist/release/pedeefe-0.3.0-windows-x64.msi
#>
param([Parameter(Mandatory)] [string] $Path)
$ErrorActionPreference = 'Stop'
$Installer = New-Object -ComObject WindowsInstaller.Installer
$Database = $Installer.OpenDatabase((Resolve-Path -LiteralPath $Path).Path, 0)

function Read-Row([string] $Sql, [int] $Columns) {
  $view = $Database.OpenView($Sql)
  try {
    [void] $view.Execute()
    $record = $view.Fetch()
    if (-not $record) { throw "MSI row missing: $Sql" }
    $values = @(for ($i = 1; $i -le $Columns; $i++) { $record.StringData($i) })
    return ,$values
  } finally { [void] $view.Close() }
}

function Assert-Equal($Actual, $Expected, [string] $What) {
  if ($Actual -cne $Expected) { throw "$What`: expected '$Expected', got '$Actual'" }
}

function Assert-NoRow([string] $Sql, [string] $What) {
  $view = $Database.OpenView($Sql)
  try {
    [void] $view.Execute()
    if ($view.Fetch()) { throw "Unexpected MSI row ($What): $Sql" }
  } finally { [void] $view.Close() }
}

# Test the compiled condition with Windows Installer's evaluator, in a restricted session that
# cannot change machine state. Normal installs/repairs work, per-user overrides fail, and removal
# of an older incorrectly scoped installation remains possible (#305).
$scopeMessage = 'PeDeeFe must be installed for all users. Run setup with administrator privileges and ALLUSERS=1; per-user installation is not supported.'
$scopeCondition = Read-Row ('SELECT `Condition` FROM `LaunchCondition` WHERE `Description` = ''' + $scopeMessage + '''') 1
$Installer.UILevel = 2
$session = $null
try {
  $session = $Installer.OpenPackage((Resolve-Path -LiteralPath $Path).Path, 1)
  foreach ($case in @(
      @('machine install', '1', '', '', '', 1),
      @('machine repair', '1', '', '1', '', 1),
      @('forced per-user install', '2', '1', '', '', 0),
      @('empty ALLUSERS', '', '', '', '', 0),
      @('resolved per-user install', '', '1', '', '', 0),
      @('forced per-user repair', '2', '1', '1', '', 0),
      @('machine uninstall', '1', '', '1', 'ALL', 1),
      @('legacy per-user uninstall', '', '1', '1', 'ALL', 1))) {
    $session.Property('ALLUSERS') = $case[1]
    $session.Property('MSIINSTALLPERUSER') = $case[2]
    $session.Property('Installed') = $case[3]
    $session.Property('REMOVE') = $case[4]
    Assert-Equal ($session.EvaluateCondition($scopeCondition[0])) $case[5] $case[0]
  }
} finally {
  if ($session) { [void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($session) }
}
foreach ($sequence in @('InstallUISequence', 'InstallExecuteSequence')) {
  $launch = Read-Row ('SELECT `Condition`, `Sequence` FROM `' + $sequence + '` WHERE `Action` = ''LaunchConditions''') 2
  Assert-Equal $launch[0] '' "$sequence launch conditions are unconditional"
  $cost = Read-Row ('SELECT `Sequence` FROM `' + $sequence + '` WHERE `Action` = ''CostInitialize''') 1
  if ([int] $launch[1] -le 0 -or [int] $launch[1] -ge [int] $cost[0]) {
    throw "$sequence must reject per-user overrides before costing"
  }
}
$manufacturer = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''Manufacturer''' 1
Assert-Equal $manufacturer[0] 'Phreezi' 'MSI manufacturer'
$status = Read-Row 'SELECT `Text` FROM `Control` WHERE `Dialog_` = ''InstallProgress'' AND `Control` = ''Status''' 1
Assert-Equal $status[0] 'Please wait while setup completes.' 'Persistent progress message'
Assert-NoRow 'SELECT `Event` FROM `EventMapping` WHERE `Dialog_` = ''InstallProgress'' AND `Control_` = ''Status''' 'progress text subscription'

# PeDeeFe's own identity (a fork of PdfCraft that installs next to an official PdfCraft): product
# name, install folder, executable name and its App Paths entry.
$productName = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''ProductName''' 1
Assert-Equal $productName[0] 'PeDeeFe' 'MSI product name'
$installDir = Read-Row 'SELECT `DefaultDir` FROM `Directory` WHERE `Directory` = ''INSTALLFOLDER''' 1
if ($installDir[0] -notmatch '(^|\|)PeDeeFe$') { throw "Install folder name: '$($installDir[0])'" }
$exe = Read-Row 'SELECT `FileName`, `Component_` FROM `File` WHERE `File` = ''PdfcraftExe''' 2
if ($exe[0] -notmatch '(^|\|)pedeefe\.exe$') { throw "Executable file name: '$($exe[0])'" }
Assert-Equal $exe[1] 'PdfcraftApp' 'Executable component'
$appPath = Read-Row 'SELECT `Value`, `Root` FROM `Registry` WHERE `Key` = ''Software\Microsoft\Windows\CurrentVersion\App Paths\pedeefe.exe'' AND `Name` IS NULL' 2
Assert-Equal $appPath[0] '[#PdfcraftExe]' 'App Paths entry'
Assert-Equal $appPath[1] '2' 'App Paths HKLM root'

# Plain (non-advertised) shortcuts to pedeefe.exe, each in its own component; the desktop one is
# gated by INSTALLDESKTOPSHORTCUT, which is secure so the UI choice reaches the
# elevated install.
foreach ($entry in @(@('StartMenuShortcut', 'ProgramMenuFolder', 'PdfcraftStartMenuShortcut', ''),
                     @('DesktopShortcut', 'DesktopFolder', 'PdfcraftDesktopShortcut', 'INSTALLDESKTOPSHORTCUT = 1'))) {
  $row = Read-Row ('SELECT `Directory_`, `Target`, `Icon_`, `WkDir`, `Component_` FROM `Shortcut` WHERE `Shortcut` = ''' + $entry[0] + '''') 5
  Assert-Equal $row[0] $entry[1] "$($entry[0]) directory"
  Assert-Equal $row[1] '[#PdfcraftExe]' "$($entry[0]) target"
  Assert-Equal $row[2] 'PdfcraftIcon.ico' "$($entry[0]) icon"
  Assert-Equal $row[3] 'INSTALLFOLDER' "$($entry[0]) working directory"
  Assert-Equal $row[4] $entry[2] "$($entry[0]) component"
  $component = Read-Row ('SELECT `Directory_`, `Condition` FROM `Component` WHERE `Component` = ''' + $entry[2] + '''') 2
  Assert-Equal $component[0] $entry[1] "$($entry[2]) directory"
  Assert-Equal $component[1] $entry[3] "$($entry[2]) condition"
}
# No static default (#1058): a SetProperty after AppSearch turns it on for a first install, and for an
# update only while the old shortcut is still on the desktop; a command-line value is left alone.
Assert-NoRow 'SELECT `Value` FROM `Property` WHERE `Property` = ''INSTALLDESKTOPSHORTCUT''' 'desktop shortcut static default'
$setDesktop = Read-Row 'SELECT `Action`, `Target` FROM `CustomAction` WHERE `Source` = ''INSTALLDESKTOPSHORTCUT''' 2
Assert-Equal $setDesktop[1] '1' 'Desktop shortcut default value'
foreach ($sequence in @('InstallUISequence', 'InstallExecuteSequence')) {
  $search = Read-Row ('SELECT `Sequence` FROM `' + $sequence + '` WHERE `Action` = ''AppSearch''') 1
  $set = Read-Row ('SELECT `Sequence`, `Condition` FROM `' + $sequence + '` WHERE `Action` = ''' + $setDesktop[0] + '''') 2
  if ([int] $set[0] -le [int] $search[0]) { throw "$sequence must set the desktop shortcut default after AppSearch" }
}
$session = $null
try {
  $session = $Installer.OpenPackage((Resolve-Path -LiteralPath $Path).Path, 1)
  foreach ($case in @(
      @('first install', '', '', '', 1),
      @('first install, INSTALLDESKTOPSHORTCUT=0', '0', '', '', 0),
      @('update, shortcut still there', '', '{00000000-0000-0000-0000-000000000000}', 'C:\Users\Public\Desktop\PeDeeFe.lnk', 1),
      @('update, shortcut deleted', '', '{00000000-0000-0000-0000-000000000000}', '', 0),
      @('update, INSTALLDESKTOPSHORTCUT=0', '0', '{00000000-0000-0000-0000-000000000000}', 'C:\Users\Public\Desktop\PeDeeFe.lnk', 0))) {
    $session.Property('INSTALLDESKTOPSHORTCUT') = $case[1]
    $session.Property('WIX_UPGRADE_DETECTED') = $case[2]
    $session.Property('EXISTINGDESKTOPSHORTCUT') = $case[3]
    Assert-Equal ($session.EvaluateCondition($set[1])) $case[4] "Desktop shortcut default: $($case[0])"
  }
} finally {
  if ($session) { [void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($session) }
}
$secure = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''SecureCustomProperties''' 1
if (($secure[0] -split ';') -notcontains 'INSTALLDESKTOPSHORTCUT') { throw "INSTALLDESKTOPSHORTCUT is not secure: '$($secure[0])'" }
$checkbox = Read-Row 'SELECT `Type`, `Property`, `Text` FROM `Control` WHERE `Dialog_` = ''InstallWelcome'' AND `Control` = ''DesktopShortcut''' 3
Assert-Equal $checkbox[0] 'CheckBox' 'Welcome desktop-shortcut control'
Assert-Equal $checkbox[1] 'INSTALLDESKTOPSHORTCUT' 'Welcome checkbox property'
if ($checkbox[2] -notmatch 'desktop shortcut') { throw "Welcome checkbox label: '$($checkbox[2])'" }
$checked = Read-Row 'SELECT `Value` FROM `CheckBox` WHERE `Property` = ''INSTALLDESKTOPSHORTCUT''' 1
Assert-Equal $checked[0] '1' 'Welcome checkbox value'
$app = Read-Row 'SELECT `KeyPath` FROM `Component` WHERE `Component` = ''PdfcraftApp''' 1
Assert-Equal $app[0] 'PdfcraftExe' 'Shortcut executable key path'
$scope = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''ALLUSERS''' 1
Assert-Equal $scope[0] '1' 'Per-machine shortcut scope'

# Image context menu opens the DPI chooser; the app component owns every registry row so
# uninstall removes it. No image default association is changed.
foreach ($ext in @('png', 'jpg', 'jpeg', 'tif', 'tiff', 'gif', 'bmp', 'jp2', 'j2k', 'jpx')) {
  $key = 'Software\Classes\SystemFileAssociations\.' + $ext + '\shell\PeDeeFe.CreatePdf'
  $menu = Read-Row ('SELECT `Value`, `Component_`, `Root` FROM `Registry` WHERE `Key` = ''' + $key + ''' AND `Name` IS NULL') 3
  Assert-Equal $menu[0] 'Create PDF with PeDeeFe…' "$ext context menu label"
  Assert-Equal $menu[1] 'PdfcraftApp' "$ext context menu component"
  Assert-Equal $menu[2] '2' "$ext context menu HKLM root"
  $command = Read-Row ('SELECT `Value` FROM `Registry` WHERE `Key` = ''' + $key + '\command''') 1
  Assert-Equal $command[0] '"[#PdfcraftExe]" --create-images "%1"' "$ext context menu command"
  $selection = Read-Row ('SELECT `Value` FROM `Registry` WHERE `Key` = ''' + $key + ''' AND `Name` = ''MultiSelectModel''') 1
  Assert-Equal $selection[0] 'Single' "$ext context menu selection"
}

# PDF verbs on PeDeeFe's ProgID and its Applications entry: open, and print / printto for
# Explorer's Print and Outlook's Quick Print (apps/pdfcraft/src/quick_print.rs). Open stays the
# default verb; the app component owns every row so uninstall removes them.
$verbs = @(@('open', '"[#PdfcraftExe]" "%1"'),
           @('print', '"[#PdfcraftExe]" --print "%1"'),
           @('printto', '"[#PdfcraftExe]" --print-to "%2" "%1" --printer-driver "%3" --printer-port "%4"'))
foreach ($root in @('Software\Classes\PeDeeFe.Document', 'Software\Classes\Applications\pedeefe.exe')) {
  foreach ($verb in $verbs) {
    $row = Read-Row ('SELECT `Value`, `Component_`, `Root` FROM `Registry` WHERE `Key` = ''' + $root + '\shell\' + $verb[0] + '\command'' AND `Name` IS NULL') 3
    Assert-Equal $row[0] $verb[1] "$root $($verb[0]) command"
    Assert-Equal $row[1] 'PdfcraftApp' "$root $($verb[0]) component"
    Assert-Equal $row[2] '2' "$root $($verb[0]) HKLM root"
  }
}
$defaultVerb = Read-Row 'SELECT `Value` FROM `Registry` WHERE `Key` = ''Software\Classes\PeDeeFe.Document\shell'' AND `Name` IS NULL' 1
Assert-Equal $defaultVerb[0] 'open' 'PDF default verb'

# Default Programs registration under PeDeeFe's own names (never PdfCraft's): the document icon,
# OpenWithProgids, Capabilities and RegisteredApplications, all owned by the app component.
foreach ($entry in @(
    @('Software\Classes\PeDeeFe.Document\DefaultIcon', $null, '[#PedeefeDocIcon]'),
    @('Software\Classes\Applications\pedeefe.exe\DefaultIcon', $null, '[#PedeefeDocIcon]'),
    @('Software\Classes\Applications\pedeefe.exe', 'FriendlyAppName', 'PeDeeFe'),
    @('Software\Classes\.pdf\OpenWithProgids', 'PeDeeFe.Document', ''),
    @('Software\PeDeeFe\Capabilities', 'ApplicationName', 'PeDeeFe'),
    @('Software\PeDeeFe\Capabilities\FileAssociations', '.pdf', 'PeDeeFe.Document'),
    @('Software\RegisteredApplications', 'PeDeeFe', 'Software\PeDeeFe\Capabilities'))) {
  $nameClause = if ($null -eq $entry[1]) { '`Name` IS NULL' } else { '`Name` = ''' + $entry[1] + '''' }
  $row = Read-Row ('SELECT `Value`, `Component_`, `Root` FROM `Registry` WHERE `Key` = ''' + $entry[0] + ''' AND ' + $nameClause) 3
  Assert-Equal $row[0] $entry[2] "$($entry[0]) $($entry[1])"
  Assert-Equal $row[1] 'PdfcraftApp' "$($entry[0]) $($entry[1]) component"
  Assert-Equal $row[2] '2' "$($entry[0]) $($entry[1]) HKLM root"
}
$docIcon = Read-Row 'SELECT `FileName`, `Component_` FROM `File` WHERE `File` = ''PedeefeDocIcon''' 2
if ($docIcon[0] -notmatch '(^|\|)pedeefe-document\.ico$') { throw "Document icon file name: '$($docIcon[0])'" }
Assert-Equal $docIcon[1] 'PdfcraftApp' 'Document icon component'

# PDF context menu (#682): "Edit with PeDeeFe" opens the file in PeDeeFe. Its own component, gated
# by INSTALLPDFCONTEXTMENU (default 1, secure, welcome checkbox), with its menu value as key path so
# uninstall removes it. Several selected PDFs each start pedeefe.exe "%1" (Document), which the
# single-instance hand-off opens as tabs. The .pdf default is never set, so the user's default app
# stays.
$pdfKey = 'Software\Classes\SystemFileAssociations\.pdf\shell\PeDeeFe.Edit'
$pdfMenu = Read-Row ('SELECT `Registry`, `Value`, `Component_`, `Root` FROM `Registry` WHERE `Key` = ''' + $pdfKey + ''' AND `Name` IS NULL') 4
Assert-Equal $pdfMenu[1] 'Edit with PeDeeFe' 'PDF context menu label'
Assert-Equal $pdfMenu[2] 'PdfcraftPdfContextMenu' 'PDF context menu component'
Assert-Equal $pdfMenu[3] '2' 'PDF context menu HKLM root'
foreach ($value in @(@('Icon', '"[#PdfcraftExe]",0'), @('MultiSelectModel', 'Document'))) {
  $row = Read-Row ('SELECT `Value`, `Component_` FROM `Registry` WHERE `Key` = ''' + $pdfKey + ''' AND `Name` = ''' + $value[0] + '''') 2
  Assert-Equal $row[0] $value[1] "PDF context menu $($value[0])"
  Assert-Equal $row[1] 'PdfcraftPdfContextMenu' "PDF context menu $($value[0]) component"
}
$pdfCommand = Read-Row ('SELECT `Value`, `Component_` FROM `Registry` WHERE `Key` = ''' + $pdfKey + '\command''') 2
Assert-Equal $pdfCommand[0] '"[#PdfcraftExe]" "%1"' 'PDF context menu command'
Assert-Equal $pdfCommand[1] 'PdfcraftPdfContextMenu' 'PDF context menu command component'
$pdfComponent = Read-Row 'SELECT `Condition`, `KeyPath` FROM `Component` WHERE `Component` = ''PdfcraftPdfContextMenu''' 2
Assert-Equal $pdfComponent[0] 'INSTALLPDFCONTEXTMENU = 1' 'PDF context menu condition'
Assert-Equal $pdfComponent[1] $pdfMenu[0] 'PDF context menu key path'
$pdfDefault = Read-Row 'SELECT `Value` FROM `Property` WHERE `Property` = ''INSTALLPDFCONTEXTMENU''' 1
Assert-Equal $pdfDefault[0] '1' 'PDF context menu default'
if (($secure[0] -split ';') -notcontains 'INSTALLPDFCONTEXTMENU') { throw "INSTALLPDFCONTEXTMENU is not secure: '$($secure[0])'" }
$pdfCheckbox = Read-Row 'SELECT `Type`, `Property`, `Text` FROM `Control` WHERE `Dialog_` = ''InstallWelcome'' AND `Control` = ''PdfContextMenu''' 3
Assert-Equal $pdfCheckbox[0] 'CheckBox' 'Welcome PDF context-menu control'
Assert-Equal $pdfCheckbox[1] 'INSTALLPDFCONTEXTMENU' 'Welcome PDF context-menu property'
if ($pdfCheckbox[2] -notmatch 'Edit with PeDeeFe') { throw "Welcome PDF context-menu label: '$($pdfCheckbox[2])'" }
$pdfChecked = Read-Row 'SELECT `Value` FROM `CheckBox` WHERE `Property` = ''INSTALLPDFCONTEXTMENU''' 1
Assert-Equal $pdfChecked[0] '1' 'Welcome PDF context-menu value'
Assert-NoRow 'SELECT `Registry` FROM `Registry` WHERE `Key` = ''Software\Classes\.pdf'' AND `Name` IS NULL' '.pdf default app'

# Negative sequences are Windows Installer's success/user-exit/failure paths. Only full UI
# shows these dialogs: an unattended /qn or /qb install must never wait for a Finish click.
foreach ($exit in @(@('InstallComplete', '-1'), @('InstallCancelled', '-2'), @('InstallFailed', '-3'))) {
  $row = Read-Row ('SELECT `Condition`, `Sequence` FROM `InstallUISequence` WHERE `Action` = ''' + $exit[0] + '''') 2
  Assert-Equal $row[0] 'UILevel = 5' "$($exit[0]) UI level"
  Assert-Equal $row[1] $exit[1] "$($exit[0]) exit path"
  $finish = Read-Row ('SELECT `Type`, `Text` FROM `Control` WHERE `Dialog_` = ''' + $exit[0] + ''' AND `Control` = ''Finish''') 2
  Assert-Equal $finish[0] 'PushButton' "$($exit[0]) Finish control"
  Assert-Equal $finish[1] '&Finish' "$($exit[0]) Finish label"
  $endDialog = Read-Row ('SELECT `Argument` FROM `ControlEvent` WHERE `Dialog_` = ''' + $exit[0] + ''' AND `Control_` = ''Finish'' AND `Event` = ''EndDialog''') 1
  Assert-Equal $endDialog[0] 'Return' "$($exit[0]) Finish event"
}
$title = Read-Row 'SELECT `Text` FROM `Control` WHERE `Dialog_` = ''InstallComplete'' AND `Control` = ''Title''' 1
if ($title[0] -notmatch 'completed successfully') { throw 'Success dialog does not confirm completion' }
# OCR models (#103): installed into models\ beside pedeefe.exe, where the app looks for them.
$modelsDir = Read-Row 'SELECT `Directory_Parent`, `DefaultDir` FROM `Directory` WHERE `Directory` = ''ModelsFolder''' 2
Assert-Equal $modelsDir[0] 'INSTALLFOLDER' 'OCR models folder parent'
# DefaultDir is `models`, or `SHORT|models` with a generated 8.3 name.
if ($modelsDir[1] -notmatch '(^|\|)models$') { throw "OCR models folder name: '$($modelsDir[1])'" }
$modelFiles = 0
$view = $Database.OpenView('SELECT `File`.`FileName` FROM `File`, `Component` WHERE `File`.`Component_` = `Component`.`Component` AND `Component`.`Directory_` = ''ModelsFolder''')
try {
  [void] $view.Execute()
  while ($record = $view.Fetch()) { if ($record.StringData(1) -match '\.rten$') { $modelFiles++ } }
} finally { [void] $view.Close() }
if ($modelFiles -lt 2) { throw "expected the OCR models (*.rten) in models\, found $modelFiles" }
$rm = Read-Row 'SELECT `Dialog` FROM `Dialog` WHERE `Dialog` = ''MsiRMFilesInUse''' 1
Assert-Equal $rm[0] 'MsiRMFilesInUse' 'Files-in-use dialog'
[void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($Database)
[void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($Installer)
Write-Output 'ok MSI: per-machine scope guard, publisher, PeDeeFe identity (name, folder, pedeefe.exe, App Paths, Default Programs keys, document icon), persistent progress text, Start Menu shortcut, optional desktop shortcut (on for a first install, kept on update only while present, checkbox), PDF open/print/printto verbs, optional PDF context menu (default on, checkbox), icon/key path, full-UI success/cancel/error and Finish controls, files-in-use dialog, OCR models'
