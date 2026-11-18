param([int]$clicks=0)
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
Add-Type @"
using System.Runtime.InteropServices;
public class S{[DllImport("user32.dll")]public static extern bool SetCursorPos(int x,int y);[DllImport("user32.dll")]public static extern void mouse_event(int f,int x,int y,int d,int e);}
"@
[S]::SetCursorPos(700,600)
for($i=0;$i -lt $clicks;$i++){[S]::mouse_event(0x800,0,0,-120,0);Start-Sleep -Milliseconds 100}
Start-Sleep 1
$b=New-Object Drawing.Bitmap(800,480);$g=[Drawing.Graphics]::FromImage($b);$g.CopyFromScreen(360,390,0,0,$b.Size);$b.Save("F:\Downloads\nvmax\nvcp-list.png")
