# diag_edge_tabs2.ps1
# 2차 진단: "탭 스트립은 있으나 TabItem=0" 원인 규명.
# 후보 검증 - (C) Chromium 지연 접근성 트리(포그라운드/대기 필요),
#            (D) 탭이 TabItem 아닌 다른 ControlType 로 노출,
#            (E) FindFirst(Tab) 가 실제 탭 스트립이 아닌 다른 요소를 잡음.
# 읽기 전용이지만 [단계 B]에서 창을 포그라운드로 전환한다(포커스 이동 발생).

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

# user32: SetForegroundWindow / ShowWindowAsync (지연 트리 유도용)
$sig = @"
using System;
using System.Runtime.InteropServices;
public static class Win32Fg {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
}
"@
Add-Type -TypeDefinition $sig

$TAuto  = [System.Windows.Automation.AutomationElement]
$TCtrl  = [System.Windows.Automation.ControlType]
$TScope = [System.Windows.Automation.TreeScope]
$TWalk  = [System.Windows.Automation.TreeWalker]
$CtrlProp  = $TAuto::ControlTypeProperty
$stripCond = New-Object System.Windows.Automation.PropertyCondition($CtrlProp, $TCtrl::Tab)
$itemCond  = New-Object System.Windows.Automation.PropertyCondition($CtrlProp, $TCtrl::TabItem)
$trueCond  = [System.Windows.Automation.Condition]::TrueCondition
$rawWalker = $TWalk::RawViewWalker

function Get-EdgeWindow {
    $procs = @(Get-Process -Name msedge -ErrorAction SilentlyContinue | Where-Object { [int64]$_.MainWindowHandle -ne 0 })
    if ($procs.Count -eq 0) { return $null }
    $hwnd = $procs[0].MainWindowHandle
    return $TAuto::FromHandle($hwnd)
}

function Show-StripReport($label, $win) {
    Write-Host ""
    Write-Host "  [$label] 탭 스트립 조사"
    $allStrips = $win.FindAll($TScope::Descendants, $stripCond)
    Write-Host "    ControlType.Tab 요소 수: $($allStrips.Count)"
    $si = 0
    foreach ($strip in $allStrips) {
        $sName = $strip.Current.Name
        Write-Host "    - Tab[$si] name='$sName'"
        $cItems = $strip.FindAll($TScope::Children,    $itemCond)
        $dItems = $strip.FindAll($TScope::Descendants, $itemCond)
        Write-Host "      TabItem Children=$($cItems.Count)  Descendants=$($dItems.Count)"
        # 스트립 하위 ControlType 히스토그램 (Descendants 전체)
        $allDesc = $strip.FindAll($TScope::Descendants, $trueCond)
        $hist = @{}
        foreach ($d in $allDesc) {
            $t = $d.Current.ControlType.ProgrammaticName
            if ($hist.ContainsKey($t)) { $hist[$t]++ } else { $hist[$t] = 1 }
        }
        Write-Host "      스트립 하위 전체 요소 수: $($allDesc.Count)"
        foreach ($k in ($hist.Keys | Sort-Object)) {
            Write-Host "        $k = $($hist[$k])"
        }
        $si++
    }
}

$win = Get-EdgeWindow
if (-not $win) { Write-Host "msedge 창 없음 - 종료"; exit }
$hwnd = $win.Current.NativeWindowHandle
Write-Host "=== 대상 창 hwnd=$hwnd  title='$($win.Current.Name)' ==="

# --- [단계 A] 포그라운드 전환 없이 (현재 앱과 동일 조건: warm-up + 350ms) ---
Write-Host ""
Write-Host "=== [단계 A] 포그라운드 전환 없음 (warm-up + 350ms) ==="
$null = $win.FindFirst($TScope::Descendants, $stripCond)
Start-Sleep -Milliseconds 350
Show-StripReport "A" $win

# --- [단계 B] 창을 포그라운드로 전환 후 재조회 (지연 트리 가설 검증) ---
Write-Host ""
Write-Host "=== [단계 B] 포그라운드 전환 후 재조회 ==="
[void][Win32Fg]::ShowWindowAsync([IntPtr]$hwnd, 9)   # SW_RESTORE
[void][Win32Fg]::SetForegroundWindow([IntPtr]$hwnd)
Start-Sleep -Milliseconds 800
# FromHandle 재취득 (전환 후 트리 갱신 반영)
$win2 = $TAuto::FromHandle([IntPtr]$hwnd)
Show-StripReport "B" $win2

# --- [단계 C] 더 긴 대기 후 한 번 더 (타이밍 민감도 확인) ---
Write-Host ""
Write-Host "=== [단계 C] 추가 대기 1500ms 후 재조회 ==="
Start-Sleep -Milliseconds 1500
$win3 = $TAuto::FromHandle([IntPtr]$hwnd)
Show-StripReport "C" $win3

Write-Host ""
Write-Host "=== 2차 진단 완료 ==="
