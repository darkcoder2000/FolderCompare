#Requires -Version 7
<#
.SYNOPSIS
    (Re)creates exampleDiff\folder1 and exampleDiff\folder2: two trees with the same layout but differences that
    cover every folder-compare and text-compare case. EXPECTED.md next to them lists the expected status per path.
.DESCRIPTION
    Timestamps, attributes, junctions and ACLs cannot live in git, so the data is generated. All times are relative
    to a fixed base date and the "random" edits use a fixed seed, so every run produces the same trees.
    Re-running deletes the old exampleDiff folder first (incl. removing the deny ACL and read-only flags).
#>
[CmdletBinding()]
param([string]$Root = (Join-Path $PSScriptRoot '..\exampleDiff'))

$ErrorActionPreference = 'Stop'
$Root = [IO.Path]::GetFullPath($Root)
$L = Join-Path $Root 'folder1'
$R = Join-Path $Root 'folder2'
$Base = [datetime]::new(2026, 1, 15, 10, 0, 0, [DateTimeKind]::Utc)
$Day = 86400
$Rng = [Random]::new(42)
$Me = "$env:USERDOMAIN\$env:USERNAME"

[Text.Encoding]::RegisterProvider([Text.CodePagesEncodingProvider]::Instance)
$Utf8 = [Text.UTF8Encoding]::new($false)
$Utf8Bom = [Text.UTF8Encoding]::new($true)
$Utf16 = [Text.UnicodeEncoding]::new($false, $true)
$Utf16Be = [Text.UnicodeEncoding]::new($true, $true)
$Ansi = [Text.Encoding]::GetEncoding(1252)

$Expected = [Collections.Generic.List[object]]::new()

# ---------------------------------------------------------------- helpers

function T([double]$Seconds) { $Base.AddSeconds($Seconds) }

function Exp([string]$Rel, [string]$Status, [string]$Note = '') {
    $Expected.Add([pscustomobject]@{ Path = $Rel; Status = $Status; Note = $Note })
}

function Put([string]$Path, $Data, [datetime]$Time, [Text.Encoding]$Enc = $Utf8) {
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Path))
    if ($Data -is [byte[]]) { [IO.File]::WriteAllBytes($Path, $Data) }
    else { [IO.File]::WriteAllText($Path, [string]$Data, $Enc) }
    [IO.File]::SetLastWriteTimeUtc($Path, $Time)
}

# Writes the left and/or right version of a file ($null = file missing on that side). Offsets in seconds from $Base.
function Pair([string]$Rel, $LData, $RData, [double]$LOff, [double]$ROff, [string]$Status, [string]$Note = '',
              [Text.Encoding]$LEnc = $Utf8, [Text.Encoding]$REnc = $null) {
    if (-not $REnc) { $REnc = $LEnc }
    if ($null -ne $LData) { Put (Join-Path $L $Rel) $LData (T $LOff) $LEnc }
    if ($null -ne $RData) { Put (Join-Path $R $Rel) $RData (T $ROff) $REnc }
    Exp $Rel $Status $Note
}

function Lines([string[]]$Lines, [string]$Eol = "`r`n", [switch]$NoFinalEol) {
    ($Lines -join $Eol) + ($NoFinalEol ? '' : $Eol)
}

$Words = @('alpha', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel', 'india', 'juliet', 'kilo',
    'lima', 'mike', 'november', 'oscar', 'papa', 'quebec', 'romeo', 'sierra', 'tango', 'uniform', 'victor',
    'whiskey', 'xray', 'yankee', 'zulu', 'folder', 'compare', 'timestamp', 'backup', 'archive', 'source', 'target',
    'report', 'invoice', 'summary', 'config', 'value', 'result', 'error', 'window', 'column', 'branch', 'merge')

function Word { $Words[$Rng.Next($Words.Count)] }
function Sentence([int]$Min = 5, [int]$Max = 12) { (1..$Rng.Next($Min, $Max + 1) | ForEach-Object { Word }) -join ' ' }

# Replaces two words of a line (keeps the first token) so the inline diff has something to highlight.
function Mutate([string]$Line) {
    $parts = $Line.Split(' ')
    foreach ($n in 1..2) {
        $i = $Rng.Next(1, $parts.Count)
        $parts[$i] = (Word).ToUpperInvariant()
    }
    $parts -join ' '
}

function Bytes([int]$Count) { $b = [byte[]]::new($Count); $Rng.NextBytes($b); $b }

# ---------------------------------------------------------------- clean up a previous run

if (Test-Path -LiteralPath $Root) {
    foreach ($side in $L, $R) {
        $locked = Join-Path $side 'error\locked'
        if (Test-Path -LiteralPath $locked) { icacls $locked /remove:d $Me /Q | Out-Null }
    }
    # Get-ChildItem -Recurse does not follow junctions in PowerShell 7, so the loop junction is harmless here.
    # Links are removed on their own first (non-recursive delete only removes the link, never the target);
    # the recursive delete below fails intermittently on junctions.
    $items = Get-ChildItem -LiteralPath $Root -Recurse -Force -ErrorAction SilentlyContinue
    $items | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint } | ForEach-Object {
        if ($_.PSIsContainer) { [IO.Directory]::Delete($_.FullName) } else { [IO.File]::Delete($_.FullName) }
    }
    $items | Where-Object { -not $_.PSIsContainer -and ($_.Attributes -band [IO.FileAttributes]::ReadOnly) } |
        ForEach-Object { $_.Attributes = 'Normal' }
    [IO.Directory]::Delete($Root, $true)
}
[void][IO.Directory]::CreateDirectory($L)
[void][IO.Directory]::CreateDirectory($R)

# ---------------------------------------------------------------- identical

$t = 'Same content and same timestamp on both sides.'
Pair 'identical\a.txt' $t $t 0 0 'Identical'
Pair 'identical\b.md' "# Notes`r`n`r`n$t`r`n" "# Notes`r`n`r`n$t`r`n" (-30 * $Day) (-30 * $Day) 'Identical'
Pair 'identical\sub\c.json' '{ "same": true }' '{ "same": true }' (-400 * $Day) (-400 * $Day) 'Identical'
Exp 'identical\' 'Identical (=)' 'Folder rolls up to = because everything inside is identical'

# ---------------------------------------------------------------- timestamps

Pair 'timestamps\within_tolerance_1s.txt' 'v1' 'v1' 0 1 'Identical' '1 s apart (default tolerance 2 s)'
Pair 'timestamps\tolerance_boundary_2s.txt' 'v1' 'v1' 0 2 'Identical' 'Exactly 2 s apart (tolerance is inclusive)'
Pair 'timestamps\just_outside_3s.txt' 'v1' 'v1' 0 3 'NewerRight' '3 s apart'
Pair 'timestamps\newer_left.txt' (Lines 'version 2', 'edited on the left') (Lines 'version 1') (3 * $Day) 0 'NewerLeft' 'Left 3 days newer, content differs'
Pair 'timestamps\newer_right.txt' (Lines 'version 1') (Lines 'version 2', 'edited on the right') 0 (3 * $Day) 'NewerRight' 'Right 3 days newer, content differs'
Pair 'timestamps\dst_one_hour.txt' 'dst' 'dst' 0 3600 'NewerRight' 'Exactly 1 h -> Identical when "Ignore 1 h DST offset" is on'
Pair 'timestamps\dst_one_hour_plus_1s.txt' 'dst' 'dst' 3601 0 'NewerLeft' '1 h 1 s -> Identical with DST option (within tolerance)'
Pair 'timestamps\dst_one_hour_plus_5s.txt' 'dst' 'dst' 0 3605 'NewerRight' '1 h 5 s -> stays NewerRight even with DST option'
Pair 'timestamps\same_time_diff_content.txt' 'short' (Lines 'much longer content', 'with a different size') 0 0 'Identical' 'Same timestamp, different size/content: time-only compare says identical'
Pair 'timestamps\diff_time_same_content.txt' 'same bytes' 'same bytes' 0 600 'NewerRight' 'Same bytes, 10 min apart'
Pair 'timestamps\future_date.txt' 'from the future' 'today' 0 ([datetime]::new(2099, 12, 31, 0, 0, 0, [DateTimeKind]::Utc) - $Base).TotalSeconds 'NewerRight' 'Right has year 2099'
Pair 'timestamps\very_old_1980.txt' 'old' 'new' ([datetime]::new(1980, 1, 1, 0, 0, 0, [DateTimeKind]::Utc) - $Base).TotalSeconds 0 'NewerRight' 'Left has 1980-01-01'

# ---------------------------------------------------------------- only on one side

Pair 'only\only_left.txt' 'left only' $null 0 0 'OnlyLeft'
Pair 'only\only_right.txt' $null 'right only' 0 0 'OnlyRight'
Pair 'only\left_only_folder\one.txt' '1' $null 0 0 'OnlyLeft'
Pair 'only\left_only_folder\nested\two.txt' '2' $null 0 0 'OnlyLeft'
Exp 'only\left_only_folder\' 'OnlyLeft' 'Whole folder tree only on the left'
Pair 'only\right_only_folder\one.txt' $null '1' 0 0 'OnlyRight'
Pair 'only\right_only_folder\nested\deeper\two.txt' $null '2' 0 0 'OnlyRight'
Exp 'only\right_only_folder\' 'OnlyRight' 'Whole folder tree only on the right'
[void][IO.Directory]::CreateDirectory((Join-Path $L 'only\empty_left_only'))
Exp 'only\empty_left_only\' 'OnlyLeft' 'Empty folder'
[void][IO.Directory]::CreateDirectory((Join-Path $R 'only\empty_right_only'))
Exp 'only\empty_right_only\' 'OnlyRight' 'Empty folder'
[void][IO.Directory]::CreateDirectory((Join-Path $L 'only\empty_both'))
[void][IO.Directory]::CreateDirectory((Join-Path $R 'only\empty_both'))
Exp 'only\empty_both\' 'Identical (=)' 'Empty on both sides'

# ---------------------------------------------------------------- type conflicts

Put (Join-Path $L 'conflicts\file_vs_folder') 'I am a file on the left' (T 0)
Put (Join-Path $R 'conflicts\file_vs_folder\inside.txt') 'I am inside a folder on the right' (T 0)
Exp 'conflicts\file_vs_folder' 'TypeConflict' 'File left, folder right'
Put (Join-Path $L 'conflicts\folder_vs_file\inside.txt') 'I am inside a folder on the left' (T 0)
Put (Join-Path $R 'conflicts\folder_vs_file') 'I am a file on the right' (T 0)
Exp 'conflicts\folder_vs_file' 'TypeConflict' 'Folder left, file right'

# ---------------------------------------------------------------- case-insensitive matching

Put (Join-Path $L 'case\ReadMe.txt') 'case' (T 0)
Put (Join-Path $R 'case\README.TXT') 'case' (T 0)
Exp 'case\ReadMe.txt' 'Identical' 'Named README.TXT on the right'
Put (Join-Path $L 'case\SubFolder\x.txt') 'x' (T 0)
Put (Join-Path $R 'case\subfolder\x.txt') 'x changed' (T $Day)
Exp 'case\SubFolder\x.txt' 'NewerRight' 'Folder is named subfolder on the right'

# ---------------------------------------------------------------- deep nesting / roll-up

Pair 'deep\a\same1.txt' 's' 's' 0 0 'Identical'
Pair 'deep\a\b\c\same2.txt' 's' 's' 0 0 'Identical'
Pair 'deep\a\b\c\d\e\f\same3.txt' 's' 's' 0 0 'Identical'
Pair 'deep\a\b\c\d\e\f\deep_change.txt' 'old' 'new' 0 (2 * $Day) 'NewerRight' 'Only difference 7 levels down: deep, a..f all roll up to ≠'
Pair 'deep\x\y\z\all_same.txt' 's' 's' 0 0 'Identical' 'Sibling branch stays ='

# ---------------------------------------------------------------- names

$emoji = [char]::ConvertFromUtf32(0x1F600)
Pair "names\Ünïcödé_日本語_émoji_$emoji.txt" 'unicode left' 'unicode' $Day 0 'NewerLeft' 'Unicode + emoji (surrogate pair) in the name'
Pair 'names\file with spaces & (parens) [x].txt' 's' 's' 0 0 'Identical'
Pair 'names\Ümlaut-Ordner\Datei.txt' 'a' 'b' 0 $Day 'NewerRight' 'Non-ASCII folder name'
Pair 'names\file.with.many.dots.tar.gz' (Bytes 64) (Bytes 64) 0 0 'Identical'
Pair 'names\Makefile' 'all:' 'all: build' 0 120 'NewerRight' 'No extension'
Pair 'names\.hiddenstyle' 'x' 'x' 0 0 'Identical' 'Leading dot'
$seg = 'a_rather_long_directory_name_used_to_exceed_MAX_PATH_0'
$long = 'names\long\' + ((1..5 | ForEach-Object { $seg + $_ }) -join '\') + '\long_path_file_with_a_long_name_as_well.txt'
Pair $long 'long' 'long changed' 0 $Day 'NewerRight' "Full path > 260 chars ($((Join-Path $L $long).Length) chars)"

# ---------------------------------------------------------------- attributes

Pair 'attributes\readonly.txt' 'old, read-only' 'newer' 0 $Day 'NewerRight' 'Left is ReadOnly: copying right->left must overwrite a read-only file'
(Get-Item -LiteralPath (Join-Path $L 'attributes\readonly.txt')).Attributes = 'ReadOnly'
Pair 'attributes\hidden.txt' 'hidden and newer' 'visible' $Day 0 'NewerLeft' 'Left is Hidden'
(Get-Item -LiteralPath (Join-Path $L 'attributes\hidden.txt') -Force).Attributes = 'Hidden'
Pair 'attributes\hidden_both.txt' 'h' 'h' 0 0 'Identical' 'Hidden on both sides'
(Get-Item -LiteralPath (Join-Path $L 'attributes\hidden_both.txt') -Force).Attributes = 'Hidden'
(Get-Item -LiteralPath (Join-Path $R 'attributes\hidden_both.txt') -Force).Attributes = 'Hidden'

# ---------------------------------------------------------------- include / exclude filters

Pair 'filters\app.log' 'log 1' 'log 2' 0 300 'NewerRight' 'Excluded by *.log'
Pair 'filters\debug.log' 'log' $null 0 0 'OnlyLeft' 'Excluded by *.log'
Pair 'filters\bin\out.dll' (Bytes 512) (Bytes 512) 600 0 'NewerLeft' 'Excluded by bin'
Pair 'filters\obj\x.cache' 'c1' 'c2' 0 600 'NewerRight' 'Excluded by obj'
Pair 'filters\keep.cs' 'class A {}' 'class A { }' 0 60 'NewerRight' 'Kept by include *.cs'
Pair 'filters\notes.md' 'n' 'n' 0 0 'Identical' 'Not matched by include *.cs'

# ---------------------------------------------------------------- sizes

Pair 'sizes\zero_bytes.txt' '' '' 0 $Day 'NewerRight' '0 bytes on both sides'
Pair 'sizes\zero_vs_some.bin' ([byte[]]::new(0)) (Bytes 1024) 0 $Day 'NewerRight' '0 bytes vs 1 KB'
$big = Bytes (5MB)
$big2 = [byte[]]$big.Clone(); foreach ($i in 0..99) { $big2[$Rng.Next($big2.Length)] = 0xFF }
Pair 'sizes\large_5MB.bin' $big $big2 $Day 0 'NewerLeft' '5 MB, for copy progress'

# ---------------------------------------------------------------- links

Pair 'links\regular.txt' 'r' 'r' 0 0 'Identical'
New-Item -ItemType Junction -Path (Join-Path $L 'links\junction_to_identical') -Target (Join-Path $L 'identical') | Out-Null
New-Item -ItemType Junction -Path (Join-Path $R 'links\junction_to_identical') -Target (Join-Path $R 'identical') | Out-Null
Exp 'links\junction_to_identical' 'Skipped (default) / Identical (Follow)' 'Junction on both sides -> identical\'
New-Item -ItemType Junction -Path (Join-Path $L 'links\loop_to_parent') -Target (Join-Path $L 'links') | Out-Null
Exp 'links\loop_to_parent' 'Skipped (default) / loop detected (Follow)' 'Left only: junction pointing at its own parent'
try {
    New-Item -ItemType SymbolicLink -Path (Join-Path $L 'links\file_symlink.txt') -Target (Join-Path $L 'links\regular.txt') | Out-Null
    New-Item -ItemType SymbolicLink -Path (Join-Path $R 'links\file_symlink.txt') -Target (Join-Path $R 'links\regular.txt') | Out-Null
    Exp 'links\file_symlink.txt' 'Skipped (default) / Identical (Follow)' 'File symlink on both sides'
}
catch {
    Write-Warning "File symlinks not created (needs Developer Mode or admin): $($_.Exception.Message)"
}

# ---------------------------------------------------------------- error (unreadable folder)

Pair 'error\fine.txt' 'f' 'f' 0 0 'Identical' 'Scan continues next to the unreadable folder'
Pair 'error\locked\secret.txt' 'secret' 'secret v2' 0 $Day 'Error' 'Right folder error\locked denies list access to the current user'

# ---------------------------------------------------------------- text compare

$chgL = 1..60 | ForEach-Object { '{0:D2} {1}' -f $_, (Sentence) }
$chgR = [Collections.Generic.List[string]]::new([string[]]$chgL)
$chgR.AddRange([string[]]@('appended line one', 'appended line two', 'appended line three'))   # right-only at end
$chgR[44] = Mutate $chgR[44]                                                                          # changed words
$chgR.InsertRange(36, [string[]]@('>>> inserted in the middle 1', '>>> inserted in the middle 2', '>>> inserted in the middle 3'))
$chgR.RemoveRange(19, 4)                                                                           # left-only block
$chgR[10] = Mutate $chgR[10]
$chgR[9] = Mutate $chgR[9]
$chgR.InsertRange(0, [string[]]@('NEW first line', 'NEW second line'))                            # right-only at start
Pair 'text\changes.txt' (Lines $chgL) (Lines $chgR) 0 $Day 'NewerRight' 'Inserted blocks at start/middle/end, deleted block, changed words'

$wl = @('int x = 1;', 'int y = 2;', "`tindented with a tab", 'Hello World', 'trailing space', 'Unchanged line', 'real change here', 'SELECT * FROM Table')
$wr = @('int  x  =  1;', 'int y = 2;    ', '    indented with a tab', 'hello world', 'trailing space   ', 'Unchanged line', 'a REAL difference here', 'select * from table')
Pair 'text\whitespace_case.txt' (Lines $wl) (Lines $wr) 0 $Day 'NewerRight' 'Only whitespace / only case diffs (blue when ignored) + one real change'

$big = 1..3000 | ForEach-Object { 'Line {0:D4}: {1}' -f $_, (Sentence 4 10) }
$bigR = [Collections.Generic.List[string]]::new([string[]]$big)
$positions = 1..40 | ForEach-Object { $Rng.Next(0, 2990) } | Sort-Object -Unique -Descending
foreach ($p in $positions) {
    switch ($Rng.Next(3)) {
        0 { $bigR[$p] = Mutate $bigR[$p] }
        1 { $bigR.InsertRange($p, [string[]](1..$Rng.Next(1, 4) | ForEach-Object { "+ inserted: $(Sentence)" })) }
        2 { $bigR.RemoveRange($p, $Rng.Next(1, 4)) }
    }
}
Pair 'text\big_random.txt' (Lines $big) (Lines $bigR) 0 $Day 'NewerRight' "3000 lines, $($positions.Count) random edits/inserts/deletes (seed 42)"

$ll = 1..20 | ForEach-Object { "$_ " + ((1..90 | ForEach-Object { Word }) -join ' ') }
$llR = [string[]]$ll.Clone()
$llR[6] = $llR[6] + ' CHANGED AT THE VERY END'
$llR[14] = $llR[14].Substring(0, 300) + ' MIDDLE ' + $llR[14].Substring(300)
Pair 'text\long_lines.txt' (Lines $ll) (Lines $llR) 0 $Day 'NewerRight' 'Lines of 500+ chars, changes far to the right'

$de = @('Grüße aus München', 'Äpfel, Öl und Übermut', 'Preis: 12,50 €', 'naïve café résumé')
$deChanged = @('Grüße aus Köln', 'Äpfel, Öl und Übermut', 'Preis: 14,90 €', 'naïve café résumé', 'ß zum Schluss')
Pair 'text\enc_utf8_bom_vs_nobom.txt' (Lines $de) (Lines $de) 0 $Day 'NewerRight' 'Same text; left UTF-8 with BOM, right without' $Utf8Bom $Utf8
Pair 'text\enc_utf8_bom.txt' (Lines $de) (Lines $deChanged) 0 $Day 'NewerRight' 'UTF-8 with BOM on both sides; save must keep the BOM' $Utf8Bom
Pair 'text\enc_utf16le.txt' (Lines $de) (Lines $deChanged) 0 $Day 'NewerRight' 'UTF-16 LE with BOM' $Utf16
Pair 'text\enc_utf16be.txt' (Lines $de) (Lines $deChanged) $Day 0 'NewerLeft' 'UTF-16 BE with BOM' $Utf16Be
Pair 'text\enc_ansi1252.txt' (Lines $de) (Lines $deChanged) 0 $Day 'NewerRight' 'ANSI (Windows-1252), no BOM' $Ansi
Pair 'text\enc_ansi_vs_utf8.txt' (Lines $de) (Lines $de) 0 $Day 'NewerRight' 'Same text; left ANSI, right UTF-8' $Ansi $Utf8

$eol = @('first', 'second', 'third', 'fourth')
Pair 'text\eol_crlf_vs_lf.txt' (Lines $eol "`r`n") (Lines $eol "`n") 0 $Day 'NewerRight' 'Same text; CRLF left, LF right'
Pair 'text\eol_lf_both.txt' (Lines $eol "`n") (Lines ($eol + 'fifth') "`n") 0 $Day 'NewerRight' 'LF on both sides; save must keep LF'
Pair 'text\eol_mixed.txt' "one`r`ntwo`nthree`r`nfour`n" "one`r`ntwo changed`nthree`r`nfour`n" 0 $Day 'NewerRight' 'Mixed CRLF/LF in one file'
Pair 'text\no_trailing_newline.txt' (Lines $eol) (Lines $eol -NoFinalEol) 0 $Day 'NewerRight' 'Right has no newline at end of file'
Pair 'text\empty_vs_content.txt' '' (Lines 'now there is', 'some content') 0 $Day 'NewerRight' 'Left is 0 bytes'
Pair 'text\text_only_left.txt' (Lines 'only on the left', 'opens against an empty pane') $null 0 0 'OnlyLeft'
Pair 'text\text_only_right.txt' $null (Lines 'only on the right') 0 0 'OnlyRight'
Pair 'text\identical_text.txt' (Lines $eol) (Lines $eol) 0 0 'Identical' 'Contents compare shows no differences'

$csL = @'
using System;

namespace Demo;

public static class Program
{
    public static int Main(string[] args)
    {
        var count = args.Length;
        Console.WriteLine($"Arguments: {count}");
        return Run(count);
    }

    private static int Run(int count)
    {
        for (int i = 0; i < count; i++)
            Console.WriteLine(i);
        return 0;
    }

    private static void Unused()
    {
        // removed on the right
    }
}
'@ -replace "`r?`n", "`r`n"
$csR = @'
using System;
using System.Linq;

namespace Demo;

public static class Program
{
    private static int Run(int argumentCount)
    {
        foreach (var i in Enumerable.Range(0, argumentCount))
            Console.WriteLine(i);
        return 0;
    }

    public static int Main(string[] args)
    {
        var argumentCount = args.Length;
        Console.WriteLine($"Arguments: {argumentCount}");
        return Run(argumentCount);
    }
}
'@ -replace "`r?`n", "`r`n"
Pair 'text\code\Program.cs' $csL $csR 0 $Day 'NewerRight' 'Renamed variable, moved method, removed method'

$jsonL = @'
{
  "name": "demo",
  "version": "1.0.0",
  "timeoutSeconds": 30,
  "features": ["compare", "copy"],
  "logging": { "level": "Information" }
}
'@
$jsonR = @'
{
  "name": "demo",
  "version": "1.1.0",
  "timeoutSeconds": 45,
  "features": ["compare", "copy", "delete"],
  "logging": { "level": "Debug", "file": "log.txt" }
}
'@
Pair 'text\code\config.json' $jsonL $jsonR $Day 0 'NewerLeft' 'Changed values'

$cssL = ".button {`n  color: #333;`n  padding: 4px 8px;`n}`n`n.header {`n  font-size: 14px;`n}`n"
$cssR = ".button {`n  color: #0066cc;`n  padding: 4px 8px;`n  border-radius: 3px;`n}`n`n.footer {`n  margin: 0;`n}`n`n.header {`n  font-size: 16px;`n}`n"
Pair 'text\code\style.css' $cssL $cssR 0 $Day 'NewerRight' 'LF line endings, added rule and property'

# A real source file from this repo's history: MainViewModel.cs from the first app commit vs. after text compare
# was added. Pinned commits keep the diff stable; the file times are the commit times.
function GitBlob([string]$Spec) {
    $psi = [Diagnostics.ProcessStartInfo]::new('git', @('-C', (Join-Path $PSScriptRoot '..'), 'show', $Spec))
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $p = [Diagnostics.Process]::Start($psi)
    $ms = [IO.MemoryStream]::new()
    $p.StandardOutput.BaseStream.CopyTo($ms)
    $p.WaitForExit()
    if ($p.ExitCode -ne 0) { throw "git show $Spec failed: $($p.StandardError.ReadToEnd())" }
    , $ms.ToArray()
}
function CommitOffset([string]$Rev) {
    $iso = git -C (Join-Path $PSScriptRoot '..') log -1 --format=%cI $Rev
    ([datetimeoffset]::Parse($iso).UtcDateTime - $Base).TotalSeconds
}
try {
    Pair 'text\code\real\MainViewModel.cs' `
        (GitBlob '97a2126:src/TimeDiff.App/ViewModels/MainViewModel.cs') `
        (GitBlob 'c6d1514:src/FolderCompare.App/ViewModels/MainViewModel.cs') `
        (CommitOffset 97a2126) (CommitOffset c6d1514) 'NewerRight' `
        'Real file from this repo (commit 97a2126 vs c6d1514): rename, fixes, new text-compare code; 9 diff sections in ~1100 lines'
}
catch {
    Write-Warning "Real source example skipped (needs git and the repo history): $($_.Exception.Message)"
}

$png1 = [Convert]::FromBase64String('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=')
$png2 = [Convert]::FromBase64String('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==')
Pair 'text\image.png' $png1 $png2 0 $Day 'NewerRight' 'Binary: Compare contents must refuse'
$bin = Bytes 4096; $bin2 = [byte[]]$bin.Clone(); $bin2[100] = 0; $bin2[2000] = 0
Pair 'text\binary.bin' $bin $bin2 $Day 0 'NewerLeft' 'Binary with NUL bytes: Compare contents must refuse'
Pair 'text\binary_disguised.txt' $bin $bin2 0 $Day 'NewerRight' 'Binary content with a .txt extension'

# ---------------------------------------------------------------- folder timestamps, lock, report

# Folder times are not compared, but fixed values keep the listing tidy. Reparse points (junctions) are skipped.
foreach ($side in $L, $R) {
    Get-ChildItem -LiteralPath $side -Recurse -Directory -Force |
        Where-Object { -not ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) } |
        ForEach-Object { $_.LastWriteTimeUtc = $Base }
}

$lockedDir = Join-Path $R 'error\locked'
icacls $lockedDir /deny "${Me}:(RX)" /Q | Out-Null
if ($LASTEXITCODE -ne 0) { Write-Warning "icacls failed ($LASTEXITCODE); error\locked is readable" }

$md = [Text.StringBuilder]::new()
[void]$md.AppendLine('# exampleDiff – expected results')
[void]$md.AppendLine()
[void]$md.AppendLine('Generated by `tools/New-ExampleDiff.ps1` (re-run it to reset). Compare `folder1` (left) with `folder2` (right)')
[void]$md.AppendLine('using the default options (tolerance 2 s, DST off, symlinks skipped, no filters) unless the note says otherwise.')
[void]$md.AppendLine('Folders that contain any difference roll up to ≠.')
[void]$md.AppendLine()
[void]$md.AppendLine('Filter test: Exclude `*.log;bin;obj` hides the log/bin/obj rows; Include `*.cs` keeps only keep.cs (and .cs files elsewhere).')
[void]$md.AppendLine()
[void]$md.AppendLine('| Path | Expected | Note |')
[void]$md.AppendLine('|---|---|---|')
foreach ($e in $Expected) { [void]$md.AppendLine("| ``$($e.Path)`` | $($e.Status) | $($e.Note) |") }
[IO.File]::WriteAllText((Join-Path $Root 'EXPECTED.md'), $md.ToString(), $Utf8)

Write-Host "Created $Root ($($Expected.Count) cases). See EXPECTED.md."
