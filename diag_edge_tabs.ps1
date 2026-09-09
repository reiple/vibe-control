# diag_edge_tabs.ps1
# Edge 탭 수집 4단계 진단 (읽기 전용): 창 발견 -> 탭 스트립 -> TabItem 수 -> 제목 수집
# 제품 코드/문서를 수정하지 않는다. UIA 조회만 수행한다.

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

# --- 타입 변수 (T 접두사) — 요소 객체 변수와 이름이 겹치지 않도록 분리 ---
$TAuto  = [System.Windows.Automation.AutomationElement]
$TCtrl  = [System.Windows.Automation.ControlType]
$TScope = [System.Windows.Automation.TreeScope]
$TWalk  = [System.Windows.Automation.TreeWalker]
$CtrlProp  = $TAuto::ControlTypeProperty
$stripCond = New-Object System.Windows.Automation.PropertyCondition($CtrlProp, $TCtrl::Tab)
$itemCond  = New-Object System.Windows.Automation.PropertyCondition($CtrlProp, $TCtrl::TabItem)
$winCond   = New-Object System.Windows.Automation.PropertyCondition($CtrlProp, $TCtrl::Window)
$rawWalker = $TWalk::RawViewWalker

# --- [1] Win32 기준: msedge 프로세스 PID 확보 ---
Write-Host "=== [1] Win32 기준 - msedge 프로세스 ==="
$edgeProcs = @(Get-Process -Name msedge -ErrorAction SilentlyContinue)
if ($edgeProcs.Count -eq 0) {
    Write-Host "  msedge 실행 안 됨 - 종료"
    exit
}
# 주의: $pid 는 PowerShell 자동 변수(현재 세션 PID)이므로 사용하지 않는다. $wPid 사용.
$edgePidSet = @($edgeProcs | Select-Object -ExpandProperty Id | Sort-Object -Unique)
$pidJoined = $edgePidSet -join ', '
Write-Host "  고유 PID 수: $($edgePidSet.Count)  ($pidJoined)"

# --- [2] UIA RootElement.FindAll(Children, Window) 에서 Edge 창 탐색 ---
Write-Host ""
Write-Host "=== [2] UIA FindAll(Children, Window) 에서 Edge 창 탐색 ==="
$root = $TAuto::RootElement
$allWins = $root.FindAll($TScope::Children, $winCond)
Write-Host "  루트 직계 창 전체 수: $($allWins.Count)"

$uiaEdgeWins = @()
foreach ($win in $allWins) {
    try {
        $wPid = $win.Current.ProcessId
        if ($edgePidSet -contains $wPid) {
            $uiaEdgeWins += $win
            $wHwnd = $win.Current.NativeWindowHandle
            $wType = $win.Current.ControlType.ProgrammaticName
            $wName = $win.Current.Name
            Write-Host "  발견 - hwnd=$wHwnd  ControlType=$wType  title='$wName'"
        }
    } catch {
        Write-Host "  창 조회 실패(무시하고 계속): $($_.Exception.Message)"
    }
}
Write-Host "  UIA FindAll(Children) 로 발견된 Edge 창: $($uiaEdgeWins.Count)개"

# --- [3] MainWindowHandle 경유 FromHandle 비교 - UIA 누락 창 확인 ---
# Get-Process.MainWindowHandle 은 프로세스당 대표 창 1개만 반환하는 한계가 있으나,
# [2] 결과와의 비교 기준으로는 유효하다. 각 창별로 예외를 격리한다.
Write-Host ""
Write-Host "=== [3] FromHandle(MainWindowHandle) 비교 - UIA 누락 창 확인 ==="
$fromHandleWins = @()
foreach ($proc in $edgeProcs) {
    $hwnd = $proc.MainWindowHandle
    if ([int64]$hwnd -eq 0) { continue }
    try {
        $el = $TAuto::FromHandle($hwnd)
        $elType = $el.Current.ControlType.ProgrammaticName
        $inUia = $uiaEdgeWins | Where-Object { [int64]$_.Current.NativeWindowHandle -eq [int64]$hwnd }
        if ($inUia) {
            Write-Host "  PID=$($proc.Id) hwnd=$hwnd ControlType=$elType -> UIA 목록에 있음"
        } else {
            Write-Host "  PID=$($proc.Id) hwnd=$hwnd ControlType=$elType -> ★ UIA FindAll(Children) 에서 누락"
            $parent = $rawWalker.GetParent($el)
            if ($parent) {
                $pType = $parent.Current.ControlType.ProgrammaticName
                $pName = $parent.Current.Name
                Write-Host "    부모 ControlType=$pType  name='$pName'"
            } else {
                Write-Host "    부모 없음 (루트 직계이어야 하는데 FindAll에 안 나옴)"
            }
        }
        $fromHandleWins += $el
    } catch {
        Write-Host "  PID=$($proc.Id) hwnd=$hwnd -> FromHandle 실패(무시하고 계속): $($_.Exception.Message)"
    }
}

# --- 조사 대상 통합 (UIA 발견분 + FromHandle 발견분, 중복 제거) ---
$allTargets = @()
foreach ($win in $uiaEdgeWins) {
    $allTargets += [PSCustomObject]@{ Element = $win; Source = "UIA" }
}
foreach ($el in $fromHandleWins) {
    $already = $allTargets | Where-Object { [int64]$_.Element.Current.NativeWindowHandle -eq [int64]$el.Current.NativeWindowHandle }
    if (-not $already) {
        $allTargets += [PSCustomObject]@{ Element = $el; Source = "FromHandle-only" }
    }
}

# --- [4] 창별 탭 스트립 + TabItem 상세 조사 ---
Write-Host ""
Write-Host "=== [4] 창별 탭 스트립 + TabItem 조사 (대상 $($allTargets.Count)개) ==="
foreach ($entry in $allTargets) {
    try {
        $el   = $entry.Element
        $src  = $entry.Source
        $hwnd = $el.Current.NativeWindowHandle
        Write-Host ""
        Write-Host "  --- hwnd=$hwnd ($src) ---"

        # Warm-up: 기존 LIST_TABS_SCRIPT 와 동일 조건 (Chromium 지연 트리 유도)
        $null = $el.FindFirst($TScope::Descendants, $stripCond)
        Start-Sleep -Milliseconds 350

        $strip = $el.FindFirst($TScope::Descendants, $stripCond)
        if (-not $strip) {
            Write-Host "  ★ Tab 스트립(ControlType.Tab) 없음 -> 탭 정보 수집 불가"
            continue
        }
        Write-Host "  Tab 스트립 발견"

        # Children vs Descendants 탐색 결과 비교
        $cItems = $strip.FindAll($TScope::Children,    $itemCond)
        $dItems = $strip.FindAll($TScope::Descendants, $itemCond)
        Write-Host "  TabItem (Children):    $($cItems.Count)개"
        Write-Host "  TabItem (Descendants): $($dItems.Count)개"

        if ($cItems.Count -eq 0 -and $dItems.Count -gt 0) {
            Write-Host "  ★ Children=0 / Descendants>0 -> 중간 컨테이너 존재 (탭 그룹 등)"
            Write-Host "  스트립 직계 자식 ControlType 목록:"
            $child = $rawWalker.GetFirstChild($strip)
            while ($child) {
                $chType = $child.Current.ControlType.ProgrammaticName
                $chName = $child.Current.Name
                Write-Host "    ControlType=$chType  name='$chName'"
                $child = $rawWalker.GetNextSibling($child)
            }
        }

        if ($dItems.Count -eq 0) {
            Write-Host "  ★ Descendants 에서도 TabItem 없음 -> 제목 수집 불가"
            continue
        }

        # 제목 수집 가능 여부 확인 (Descendants 기준)
        Write-Host "  제목 목록 (Descendants 기준):"
        $i = 0
        foreach ($t in $dItems) {
            $n = $t.Current.Name
            if ($n -eq "") { $shown = "(빈 문자열)" } else { $shown = "'$n'" }
            Write-Host "    [$i] $shown"
            $i++
        }
    } catch {
        Write-Host "  ★ 이 창 조사 실패(무시하고 계속): $($_.Exception.Message)"
        continue
    }
}

Write-Host ""
Write-Host "=== 진단 완료 ==="
