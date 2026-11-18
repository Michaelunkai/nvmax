# nvmax UI driver: NVIDIA Control Panel > Manage 3D settings > Global; sets rows via mouse, presses Apply.
# usage: ui.ps1 -Set "Low Latency Mode=Ultra","Monitor Technology=Fixed Refresh" [-Shot path] [-Fresh]
param([string[]]$Set=@(),[string]$Shot="",[switch]$Fresh)
$env:LIB=$null; $env:INCLUDE=$null
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing,System.Windows.Forms
Add-Type @"
using System.Runtime.InteropServices;
public class U{[DllImport("user32.dll")]public static extern bool SetProcessDPIAware();[DllImport("user32.dll")]public static extern bool ShowWindow(System.IntPtr h,int c);[DllImport("user32.dll")]public static extern bool SetForegroundWindow(System.IntPtr h);[DllImport("user32.dll")]public static extern bool SetCursorPos(int x,int y);[DllImport("user32.dll")]public static extern void mouse_event(int f,int x,int y,int d,int e);}
"@
# NOTE: stay DPI-unaware: all coordinates below are in the 2560x1440 logical space (screen is 3840x2160 at 150%).
$AE=[Windows.Automation.AutomationElement]; $TF=[Windows.Automation.TreeScope]::Descendants
function Win{ $p=Get-Process nvcplui -ea 0|select -First 1; if(-not $p){return $null}; $AE::RootElement.FindFirst([Windows.Automation.TreeScope]::Children,(New-Object Windows.Automation.PropertyCondition($AE::ProcessIdProperty,$p.Id))) }
function Find($root,$name,$type){ $c=New-Object Windows.Automation.PropertyCondition($AE::NameProperty,$name); foreach($e in $root.FindAll($TF,$c)){ if($e.Current.ControlType.ProgrammaticName -eq $type){return $e} }; return $null }
function FindOld($root,$name,$type){ foreach($e in $root.FindAll($TF,[Windows.Automation.Condition]::TrueCondition)){ if($e.Current.Name -eq $name -and $e.Current.ControlType.ProgrammaticName -eq $type){return $e} } }
function Tap($x,$y){ [U]::SetCursorPos([int]$x,[int]$y)|Out-Null; Start-Sleep -Milliseconds 60; [U]::mouse_event(2,0,0,0,0); [U]::mouse_event(4,0,0,0,0) }
if($Fresh){ Get-Process nvcplui -ea 0|Stop-Process -Force; Start-Sleep 2 }
if(-not (Get-Process nvcplui -ea 0)){ $pk=Get-AppxPackage NVIDIACorp.NVIDIAControlPanel|select -First 1; Start-Process (Join-Path $pk.InstallLocation "nvcplui.exe"); Start-Sleep 3 }
$bw=$null; for($n=0;$n -lt 12;$n++){ $pp=Get-Process nvcplui -ea 0|Where-Object{$_.MainWindowHandle -ne 0}|select -First 1; if($pp){ $h=$pp.MainWindowHandle; [U]::ShowWindow($h,3)|Out-Null; [U]::SetForegroundWindow($h)|Out-Null; Start-Sleep -Milliseconds 300; $w=Win; if($w){ $bw=$w.Current.BoundingRectangle; if($bw.Width -ge 2400){break} } } else { Start-Sleep 1 } }
if(-not $bw -or $bw.Width -lt 2400){ "ABORT window not maximized w=$($bw.Width) procs=$((Get-Process nvcplui -ea 0|%{$_.MainWindowHandle}) -join ',')"; exit 3 }
[System.Windows.Forms.SendKeys]::SendWait('{ESC}')
$rows='Image Scaling','Ambient Occlusion','Anisotropic filtering','Antialiasing - FXAA','Antialiasing - Gamma correction','Antialiasing - Mode','Antialiasing - Setting','Antialiasing - Transparency','Background Application Max Frame Rate','CUDA - GPUs','CUDA - Sysmem Fallback Policy','DSR - Factors','DSR - Smoothness','Low Latency Mode','Max Frame Rate','Monitor Technology'
$opts=@{'CUDA - Sysmem Fallback Policy'=@('Driver Default','Prefer No Sysmem Fallback','Prefer Sysmem Fallback');'Low Latency Mode'=@('Off','On','Ultra');'Monitor Technology'=@('Fixed Refresh','G-SYNC Compatible','G-SYNC')}
Tap 114 186; Start-Sleep -Milliseconds 1200
Tap 416 334; Start-Sleep -Milliseconds 600  # force Global Settings tab (panel reopens on the last used tab)
foreach($kv in ($Set -split ";")){
  $row,$val=$kv -split '=',2
  $idx=[array]::IndexOf($rows,$row); if($idx -lt 0){ "MISS row $row"; continue }
  $oi=[array]::IndexOf($opts[$row],$val); if($oi -lt 0){ "MISS value $val for $row"; continue }
  $ry=431+23*$idx
  for($try=0;$try -lt 3;$try++){
  Tap 450 $ry; Start-Sleep -Milliseconds 300
  Tap 1048 $ry; Start-Sleep -Milliseconds 300
  Tap 760 ($ry+24+22.5*$oi); Start-Sleep -Milliseconds 300
  # the open row editor is a Pane (or ComboBox) named with the shown value, sitting at the row's y
  $cur=$null; for($q=0;$q -lt 6 -and -not $cur;$q++){ foreach($e in (Win).FindAll($TF,[Windows.Automation.Condition]::TrueCondition)){ $r=$e.Current.BoundingRectangle; if([math]::Abs($r.Top-($ry-12)) -lt 14 -and $r.Left -gt 600 -and $r.Left -lt 800 -and $opts[$row] -contains $e.Current.Name){ $cur=$e.Current.Name; break } }; if(-not $cur){ Start-Sleep 1 } }
    if($cur -eq $val){break}; Start-Sleep -Milliseconds 500
  }
  if($cur -eq $val){ "SET $row = $val" } else { "FAILED $row wanted $val, panel shows '$cur'"; $failed=$true }
}
if($Set.Count){
  # Apply bar enters the UIA tree some seconds after a change: poll for it
  $done=$false
  for($n=0;$n -lt 70 -and -not $done;$n++){
    $ap=$null; $pn=$null
    if($n -eq 0){ [System.Windows.Forms.SendKeys]::SendWait('{TAB}'); Start-Sleep 1 }
    foreach($e in (Win).FindAll($TF,[Windows.Automation.Condition]::TrueCondition)){ if($e.Current.Name -eq 'Apply'){ if($e.Current.ControlType.ProgrammaticName -eq 'ControlType.Button'){$ap=$e;break} else {$pn=$e} } }
    if($ap){ $ap.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke(); 'APPLIED'; $done=$true; Start-Sleep 4 }
    elseif($pn){ # Apply is a plain Pane (no patterns): click its centre; UIA rect is physical, taps are logical
      $r=$pn.Current.BoundingRectangle; $k=[Windows.Forms.Screen]::PrimaryScreen.Bounds.Width/$AE::RootElement.Current.BoundingRectangle.Width
      Tap (($r.Left+$r.Width/2)*$k) (($r.Top+$r.Height/2)*$k); 'APPLIED (pane click)'; $done=$true; Start-Sleep 4 }
    else { Start-Sleep -Milliseconds 300 }
  }
  # no Apply bar and the row reads the wanted value = it was already stored
  if(-not $done){ if($failed){ exit 4 } else { 'UNCHANGED (already stored)' } }
}
if($Shot){ $s=New-Object Drawing.Bitmap(2560,1440); $g=[Drawing.Graphics]::FromImage($s); $g.CopyFromScreen(0,0,0,0,$s.Size); $s.Save($Shot) }




