using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;

namespace CitronClicker
{
    public class AppConfig
    {
        public ClickerProfile LeftProfile { get; set; } = new ClickerProfile { IsLeft = true, Hotkey = 0x56 };
        public ClickerProfile RightProfile { get; set; } = new ClickerProfile { IsLeft = false, Hotkey = 0 };
    }

    public class ClickerProfile
    {
        public bool IsLeft { get; set; }
        public bool IsEnabled { get; set; }
        public int MinCps { get; set; } = 10;
        public int MaxCps { get; set; } = 14;
        public int SuspendKey { get; set; } = 0;
        public bool AvoidGui { get; set; } = true;
        public bool Jitter { get; set; } = false;
        public int JitterIntensity { get; set; } = 2;
        public int Hotkey { get; set; } = 0;
    }

    public partial class MainWindow : Window
    {
        private const int WH_MOUSE_LL = 14;
        private const int WM_LBUTTONDOWN = 0x0201;
        private const int WM_LBUTTONUP = 0x0202;
        private const int WM_MOUSEMOVE = 0x0200;
        private const uint LLMHF_INJECTED = 0x00000001;
        private const uint MOUSEEVENTF_MOVE = 0x0001;
        private const int INPUT_MOUSE = 0;

        /// <summary>Serializes synthetic mouse from the two clicker tasks; interleaved mouse_event/SendInput can destabilize UWP (Bedrock).</summary>
        private static readonly object SyntheticMouseLock = new object();

        [StructLayout(LayoutKind.Sequential)]
        private struct MOUSEINPUT
        {
            public int dx;
            public int dy;
            public uint mouseData;
            public uint dwFlags;
            public uint time;
            public IntPtr dwExtraInfo;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct INPUT
        {
            public uint type;
            public MOUSEINPUT mi;
        }

        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

        [StructLayout(LayoutKind.Sequential)]
        private struct MSLLHOOKSTRUCT
        {
            public int pt_x;
            public int pt_y;
            public uint mouseData;
            public uint flags;
            public uint time;
            public IntPtr dwExtraInfo;
        }

        private delegate IntPtr LowLevelMouseProc(int nCode, IntPtr wParam, IntPtr lParam);

        [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
        private static extern IntPtr SetWindowsHookEx(int idHook, LowLevelMouseProc lpfn, IntPtr hMod, uint dwThreadId);

        [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool UnhookWindowsHookEx(IntPtr hhk);

        [DllImport("user32.dll", CharSet = CharSet.Auto, SetLastError = true)]
        private static extern IntPtr CallNextHookEx(IntPtr hhk, int nCode, IntPtr wParam, IntPtr lParam);

        [DllImport("kernel32.dll", CharSet = CharSet.Auto, SetLastError = true)]
        private static extern IntPtr GetModuleHandle(string lpModuleName);

        private const int WM_RBUTTONDOWN = 0x0204;
        private const int WM_RBUTTONUP = 0x0205;
        private const uint MOUSEEVENTF_RIGHTDOWN = 0x0008;
        private const uint MOUSEEVENTF_RIGHTUP = 0x0010;

        private bool physicalRmbDown = false;
        private long lastPhysicalMouseMoveTime = 0;
        private bool avoidGui = true;
        private bool isRightClick = false;

        [StructLayout(LayoutKind.Sequential)]
        public struct POINT
        {
            public int x;
            public int y;
        }

        [StructLayout(LayoutKind.Sequential)]
        public struct CURSORINFO
        {
            public int cbSize;
            public int flags;
            public IntPtr hCursor;
            public POINT ptScreenPos;
        }

        [DllImport("user32.dll")]
        public static extern bool GetCursorInfo(out CURSORINFO pci);

        [DllImport("user32.dll")]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetCursorPos(out POINT lpPoint);

        private const int CURSOR_SHOWING = 0x00000001;

        private ClickerProfile leftProfile = new ClickerProfile { IsLeft = true, Hotkey = 0x56 /* V */ };
        private ClickerProfile rightProfile = new ClickerProfile { IsLeft = false, Hotkey = 0 };
        private ClickerProfile currentProfile;
        private bool isUpdatingUI = false;

        private string GetConfigPath()
        {
            string appData = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            string folder = Path.Combine(appData, "CitronClicker");
            if (!Directory.Exists(folder)) Directory.CreateDirectory(folder);
            return Path.Combine(folder, "config.json");
        }

        private void SaveConfig(bool showDialogOnSuccess = false)
        {
            try
            {
                var config = new AppConfig { LeftProfile = leftProfile, RightProfile = rightProfile };
                string json = JsonSerializer.Serialize(config, new JsonSerializerOptions { WriteIndented = true });
                File.WriteAllText(GetConfigPath(), json);
                if (showDialogOnSuccess)
                {
                    MessageBox.Show("Configuration Saved!", "Citron Clicker", MessageBoxButton.OK, MessageBoxImage.Information);
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show("Error saving config: " + ex.Message, "Citron Clicker", MessageBoxButton.OK, MessageBoxImage.Error);
            }
        }

        private void LoadConfig()
        {
            try
            {
                string path = GetConfigPath();
                if (File.Exists(path))
                {
                    string json = File.ReadAllText(path);
                    var config = JsonSerializer.Deserialize<AppConfig>(json);
                    if (config != null)
                    {
                        if (config.LeftProfile != null) leftProfile = config.LeftProfile;
                        if (config.RightProfile != null) rightProfile = config.RightProfile;
                    }
                }
            }
            catch { }
        }

        private bool IsCursorVisible()
        {
            CURSORINFO pci = new CURSORINFO();
            pci.cbSize = Marshal.SizeOf(typeof(CURSORINFO));
            if (GetCursorInfo(out pci))
            {
                return (pci.flags & CURSOR_SHOWING) != 0;
            }
            return false;
        }

        private LowLevelMouseProc _proc;
        private IntPtr _hookID = IntPtr.Zero;
        private bool physicalLmbDown = false;

        [DllImport("user32.dll")]
        private static extern short GetAsyncKeyState(int vKey);

        [DllImport("user32.dll")]
        private static extern IntPtr GetForegroundWindow();

        [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        private static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        private static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

        private const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
        private const uint MOUSEEVENTF_LEFTUP = 0x0004;
        private const int VK_LBUTTON = 0x01;

        private bool isBindingHotkey = false;

        private CancellationTokenSource cts;
        private readonly Random random = Random.Shared;

        private DispatcherTimer uiTimer;
        private bool isMinecraftRunning = false;

        public MainWindow()
        {
            AppDomain.CurrentDomain.UnhandledException += (s, e) =>
            {
                MessageBox.Show("Fatal Error: " + e.ExceptionObject.ToString(), "Citron Clicker Crash", MessageBoxButton.OK, MessageBoxImage.Error);
            };

            if (Application.Current != null)
            {
                Application.Current.DispatcherUnhandledException += (s, e) =>
                {
                    MessageBox.Show("UI Error: " + e.Exception.ToString(), "Citron Clicker Crash", MessageBoxButton.OK, MessageBoxImage.Error);
                    e.Handled = true;
                };
            }

            LoadConfig();

            currentProfile = leftProfile;
            
            isUpdatingUI = true;
            InitializeComponent();

            isUpdatingUI = false;
            
            this.Loaded += MainWindow_Loaded;

            uiTimer = new DispatcherTimer();
            uiTimer.Interval = TimeSpan.FromMilliseconds(500);
            uiTimer.Tick += UiTimer_Tick;
            uiTimer.Start();

            cts = new CancellationTokenSource();
            Task.Run(() => ClickerLoop(leftProfile, cts.Token));
            Task.Run(() => ClickerLoop(rightProfile, cts.Token));
            Task.Run(() => HotkeyLoop(cts.Token));

            UpdateUIFromProfile();
        }

        private void MainWindow_Loaded(object sender, RoutedEventArgs e)
        {
            _proc = HookCallback;
            _hookID = SetHook(_proc);
        }

        private string VirtualKeyToString(int vk)
        {
            switch (vk)
            {
                case 0: return "None";
                case 0x01: return "Left Click";
                case 0x02: return "Right Click";
                case 0x04: return "Middle Click";
                case 0x05: return "Mouse 4";
                case 0x06: return "Mouse 5";
                default: return ((Key)KeyInterop.KeyFromVirtualKey(vk)).ToString();
            }
        }

        private int MouseButtonToVirtualKey(MouseButton button)
        {
            switch (button)
            {
                case MouseButton.Left: return 0x01;
                case MouseButton.Right: return 0x02;
                case MouseButton.Middle: return 0x04;
                case MouseButton.XButton1: return 0x05;
                case MouseButton.XButton2: return 0x06;
                default: return 0;
            }
        }

        private void UpdateUIFromProfile()
        {
            isUpdatingUI = true;
            MinCpsSlider.Value = currentProfile.MinCps;
            MaxCpsSlider.Value = currentProfile.MaxCps;
            MinCpsText.Text = currentProfile.MinCps.ToString();
            MaxCpsText.Text = currentProfile.MaxCps.ToString();
            AvoidGuiCheck.IsChecked = currentProfile.AvoidGui;
            JitterCheck.IsChecked = currentProfile.Jitter;
            JitterSlider.Value = currentProfile.JitterIntensity;
            JitterText.Text = currentProfile.JitterIntensity.ToString();
            MasterToggleCheck.IsChecked = currentProfile.IsEnabled;

            JitterIntensityCard.Visibility = currentProfile.Jitter ? Visibility.Visible : Visibility.Collapsed;

            SuspendKeyBtn.Content = VirtualKeyToString(currentProfile.SuspendKey);
            HotkeyBtn.Content = VirtualKeyToString(currentProfile.Hotkey);

            UpdateStatusText();
            isUpdatingUI = false;
        }

        private void LeftTab_Click(object sender, RoutedEventArgs e)
        {
            currentProfile = leftProfile;
            UpdateUIFromProfile();
        }

        private void RightTab_Click(object sender, RoutedEventArgs e)
        {
            currentProfile = rightProfile;
            UpdateUIFromProfile();
        }

        private static bool IsJavaLikeProcessName(string procName)
        {
            return string.Equals(procName, "java", StringComparison.OrdinalIgnoreCase)
                || string.Equals(procName, "javaw", StringComparison.OrdinalIgnoreCase);
        }

        /// <summary>Third-party / official launcher windows — not the Java or Bedrock game client.</summary>
        private static bool TitleLooksLikeMinecraftLauncher(string title)
        {
            if (string.IsNullOrWhiteSpace(title)) return false;
            var t = title.ToLowerInvariant();
            if (t.Contains("hello minecraft")) return true; // HMCL
            if (t.Contains("minecraft launcher")) return true; // official Java launcher shell
            if (t.Contains("prism launcher")) return true;
            if (t.Contains("polymc")) return true;
            if (t.Contains("multimc")) return true;
            if (t.Contains("curseforge")) return true;
            if (t.Contains("curse forge")) return true;
            if (t.Contains("gdlauncher")) return true;
            if (t.Contains("modrinth")) return true;
            if (t.Contains("tlauncher")) return true;
            if (t.Contains("ftb app")) return true;
            if (t.Contains("atlauncher")) return true;
            if (t.Contains("xmplauncher")) return true;
            if (t.Contains("pcl2")) return true;
            if (t.Contains("bmcl")) return true;
            if (t.Contains("winterslash")) return true;
            return false;
        }

        private static bool ProcessExeLooksLikeLauncherHost(Process p)
        {
            try
            {
                string? path = p.MainModule?.FileName;
                if (string.IsNullOrEmpty(path)) return false;
                var lower = path.ToLowerInvariant();
                if (lower.Contains("prismlauncher")) return true;
                if (lower.Contains("polymc")) return true;
                if (lower.Contains("multimc")) return true;
                if (lower.Contains("hmcl")) return true;
                if (lower.Contains("pcl")) return true;
                if (lower.Contains("curseforge")) return true;
                if (lower.Contains("gdlauncher")) return true;
                if (lower.Contains("modrinth")) return true;
                if (lower.Contains("tlauncher")) return true;
            }
            catch { }
            return false;
        }

        private static bool TitleSuggestsMinecraftJava(string title)
        {
            if (string.IsNullOrWhiteSpace(title)) return false;
            if (TitleLooksLikeMinecraftLauncher(title)) return false;
            var t = title.ToLowerInvariant();
            if (t.Contains("minecraft")) return true;
            if (t.Contains("lunar client")) return true;
            if (t.Contains("badlion")) return true;
            if (t.Contains("feather")) return true;
            if (t.Contains("labymod")) return true;
            if (t.Contains("salwyrr")) return true;
            if (t.Contains("fml earlyloading")) return true;
            if (t.Contains("forge")) return true;
            if (t.Contains("fabric")) return true;
            if (t.Contains("neoforge")) return true;
            return false;
        }

        /// <summary>True if this HWND is the actual Java game client (not HMCL / Prism / etc.). GLFW/LWJGL is the strongest signal.</summary>
        private static bool WindowLooksLikeMinecraftJavaClient(IntPtr hWnd, Process? owningProcess = null)
        {
            if (hWnd == IntPtr.Zero) return false;
            var titleSb = new StringBuilder(512);
            GetWindowText(hWnd, titleSb, titleSb.Capacity);
            string title = titleSb.ToString();
            if (TitleLooksLikeMinecraftLauncher(title))
                return false;

            var clsSb = new StringBuilder(256);
            string cls = "";
            if (GetClassName(hWnd, clsSb, clsSb.Capacity) > 0)
                cls = clsSb.ToString();

            bool glfwOrLwjgl = cls.Contains("GLFW", StringComparison.OrdinalIgnoreCase)
                || cls.Contains("LWJGL", StringComparison.OrdinalIgnoreCase);
            if (glfwOrLwjgl)
            {
                if (owningProcess != null && ProcessExeLooksLikeLauncherHost(owningProcess))
                    return false;
                return true;
            }

            return TitleSuggestsMinecraftJava(title);
        }

        private static bool BedrockWindowLooksLikeGame(Process p)
        {
            if (p.MainWindowHandle == IntPtr.Zero) return false;
            var titleSb = new StringBuilder(512);
            GetWindowText(p.MainWindowHandle, titleSb, titleSb.Capacity);
            if (TitleLooksLikeMinecraftLauncher(titleSb.ToString()))
                return false;
            return true;
        }

        private void UiTimer_Tick(object? sender, EventArgs e)
        {
            bool found = false;
            try
            {
                foreach (var p in Process.GetProcessesByName("Minecraft.Windows"))
                {
                    try
                    {
                        if (BedrockWindowLooksLikeGame(p)) { found = true; break; }
                    }
                    catch { }
                }
                if (!found)
                {
                    foreach (var p in Process.GetProcessesByName("Minecraft"))
                    {
                        try
                        {
                            if (BedrockWindowLooksLikeGame(p)) { found = true; break; }
                        }
                        catch { }
                    }
                }
                if (!found)
                {
                    foreach (string procName in new[] { "javaw", "java" })
                    {
                        foreach (var p in Process.GetProcessesByName(procName))
                        {
                            try
                            {
                                if (p.MainWindowHandle != IntPtr.Zero
                                    && WindowLooksLikeMinecraftJavaClient(p.MainWindowHandle, p))
                                {
                                    found = true;
                                    break;
                                }
                            }
                            catch { }
                        }
                        if (found) break;
                    }
                }
            }
            catch { }

            isMinecraftRunning = found;
            UpdateStatusText();
        }

        private void UpdateStatusText()
        {
            if (!isMinecraftRunning)
            {
                StatusText.Text = "WAITING FOR MC";
                StatusText.Foreground = new SolidColorBrush(Color.FromRgb(173, 170, 170)); // #adaaaa
                StatusDot.Fill = new SolidColorBrush(Color.FromRgb(173, 170, 170));
            }
            else
            {
                StatusText.Text = "INJECTED";
                StatusText.Foreground = new SolidColorBrush(Color.FromRgb(228, 242, 101)); // #E4F265
                StatusDot.Fill = new SolidColorBrush(Color.FromRgb(228, 242, 101));
            }
        }

        private IntPtr SetHook(LowLevelMouseProc proc)
        {
            try
            {
                using (Process curProcess = Process.GetCurrentProcess())
                using (ProcessModule curModule = curProcess.MainModule)
                {
                    IntPtr handle = GetModuleHandle(curModule?.ModuleName);
                    if (handle == IntPtr.Zero) handle = GetModuleHandle(null);
                    return SetWindowsHookEx(WH_MOUSE_LL, proc, handle, 0);
                }
            }
            catch
            {
                return IntPtr.Zero;
            }
        }

        private IntPtr HookCallback(int nCode, IntPtr wParam, IntPtr lParam)
        {
            if (nCode >= 0)
            {
                MSLLHOOKSTRUCT hookStruct = (MSLLHOOKSTRUCT)Marshal.PtrToStructure(lParam, typeof(MSLLHOOKSTRUCT));
                if ((hookStruct.flags & LLMHF_INJECTED) == 0)
                {
                    if (wParam == (IntPtr)WM_LBUTTONDOWN)
                    {
                        physicalLmbDown = true;
                    }
                    else if (wParam == (IntPtr)WM_LBUTTONUP)
                    {
                        physicalLmbDown = false;
                    }
                    else if (wParam == (IntPtr)WM_RBUTTONDOWN)
                    {
                        physicalRmbDown = true;
                    }
                    else if (wParam == (IntPtr)WM_RBUTTONUP)
                    {
                        physicalRmbDown = false;
                    }
                    else if (wParam == (IntPtr)WM_MOUSEMOVE)
                    {
                        lastPhysicalMouseMoveTime = Environment.TickCount64;
                    }
                }
            }
            return CallNextHookEx(_hookID, nCode, wParam, lParam);
        }

        private void Slider_ValueChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
        {
            if (isUpdatingUI) return;
            if (MinCpsSlider == null || MaxCpsSlider == null) return;

            if (MinCpsSlider.Value > MaxCpsSlider.Value)
            {
                if (sender == MinCpsSlider) MaxCpsSlider.Value = MinCpsSlider.Value;
                else MinCpsSlider.Value = MaxCpsSlider.Value;
            }

            currentProfile.MinCps = (int)MinCpsSlider.Value;
            currentProfile.MaxCps = (int)MaxCpsSlider.Value;

            if (MinCpsText != null) MinCpsText.Text = currentProfile.MinCps.ToString();
            if (MaxCpsText != null) MaxCpsText.Text = currentProfile.MaxCps.ToString();
            SaveConfig();
        }

        private void ToggleBtn_Click(object sender, RoutedEventArgs e)
        {
            ToggleClicker(currentProfile);
        }

        private void ToggleClicker(ClickerProfile profile = null)
        {
            if (profile == null) profile = currentProfile;
            profile.IsEnabled = !profile.IsEnabled;
            
            if (profile == currentProfile)
            {
                Dispatcher.Invoke(() =>
                {
                    MasterToggleCheck.IsChecked = profile.IsEnabled;
                    UpdateStatusText();
                });
            }
            else
            {
                Dispatcher.Invoke(UpdateStatusText);
            }
            SaveConfig();
        }

        private void MinimizeBtn_Click(object sender, RoutedEventArgs e)
        {
            this.WindowState = WindowState.Minimized;
        }

        private void MasterToggleCheck_Click(object sender, RoutedEventArgs e)
        {
            if (isUpdatingUI) return;
            if (MasterToggleCheck.IsChecked == true && !currentProfile.IsEnabled)
            {
                ToggleClicker(currentProfile);
            }
            else if (MasterToggleCheck.IsChecked == false && currentProfile.IsEnabled)
            {
                ToggleClicker(currentProfile);
            }
        }

        private void Window_MouseLeftButtonDown(object sender, MouseButtonEventArgs e)
        {
            if (e.ChangedButton == MouseButton.Left)
            {
                this.DragMove();
            }
        }

        private void CloseBtn_Click(object sender, RoutedEventArgs e)
        {
            this.Close();
        }

        private void SaveBtn_Click(object sender, RoutedEventArgs e)
        {
            SaveConfig(showDialogOnSuccess: true);
        }

        private void HotkeyBtn_Click(object sender, RoutedEventArgs e)
        {
            if (isBindingHotkey) return;
            isBindingHotkey = true;
            HotkeyBtn.Content = "Press Key...";
            this.KeyDown += OnModuleHotkeyDown;
            this.MouseDown += OnModuleHotkeyMouseDown;
            this.Focus();
        }

        private void OnModuleHotkeyDown(object sender, KeyEventArgs e)
        {
            this.KeyDown -= OnModuleHotkeyDown;
            this.MouseDown -= OnModuleHotkeyMouseDown;
            isBindingHotkey = false;

            if (e.Key == Key.Escape)
            {
                currentProfile.Hotkey = 0;
                HotkeyBtn.Content = "None";
            }
            else
            {
                int virtualKey = KeyInterop.VirtualKeyFromKey(e.Key);
                currentProfile.Hotkey = virtualKey;
                HotkeyBtn.Content = e.Key.ToString();
            }
            e.Handled = true;
            SaveConfig();
        }

        private void OnModuleHotkeyMouseDown(object sender, MouseButtonEventArgs e)
        {
            this.KeyDown -= OnModuleHotkeyDown;
            this.MouseDown -= OnModuleHotkeyMouseDown;
            isBindingHotkey = false;

            int vk = MouseButtonToVirtualKey(e.ChangedButton);
            if (vk != 0)
            {
                currentProfile.Hotkey = vk;
                HotkeyBtn.Content = VirtualKeyToString(vk);
            }
            e.Handled = true;
            SaveConfig();
        }

        private bool IsMinecraftActive()
        {
            IntPtr hWnd = GetForegroundWindow();
            if (hWnd == IntPtr.Zero) return false;

            GetWindowThreadProcessId(hWnd, out uint pid);
            try
            {
                using Process proc = Process.GetProcessById((int)pid);
                string procName = proc.ProcessName;

                if (procName.Equals("Minecraft.Windows", StringComparison.OrdinalIgnoreCase)
                    || procName.Equals("Minecraft", StringComparison.OrdinalIgnoreCase))
                {
                    var titleSb = new StringBuilder(512);
                    GetWindowText(hWnd, titleSb, titleSb.Capacity);
                    return !TitleLooksLikeMinecraftLauncher(titleSb.ToString());
                }

                if (!IsJavaLikeProcessName(procName))
                    return false;

                return WindowLooksLikeMinecraftJavaClient(hWnd, proc);
            }
            catch { }

            return false;
        }

        private static bool IsForegroundBedrock()
        {
            IntPtr hWnd = GetForegroundWindow();
            if (hWnd == IntPtr.Zero) return false;
            GetWindowThreadProcessId(hWnd, out uint pid);
            try
            {
                using Process proc = Process.GetProcessById((int)pid);
                return proc.ProcessName.Equals("Minecraft.Windows", StringComparison.OrdinalIgnoreCase)
                    || proc.ProcessName.Equals("Minecraft", StringComparison.OrdinalIgnoreCase);
            }
            catch
            {
                return false;
            }
        }

        /// <summary>Bedrock (UWP) is sensitive to ultra-tight synthetic click phases and deprecated mouse_event injection.</summary>
        private static void ClampBedrockDelays(ref int upTime, ref int downTime)
        {
            const int minPhaseMs = 8;
            const int minFrameMs = 24;
            upTime = Math.Max(upTime, minPhaseMs);
            downTime = Math.Max(downTime, minPhaseMs);
            int frame = upTime + downTime;
            if (frame < minFrameMs)
                downTime += minFrameMs - frame;
        }

        private static readonly int InputSizeBytes = Marshal.SizeOf(typeof(INPUT));

        private static void SendSyntheticMouse(uint dwFlags, int dx = 0, int dy = 0)
        {
            var mi = new MOUSEINPUT
            {
                dx = dx,
                dy = dy,
                mouseData = 0,
                dwFlags = dwFlags,
                time = 0,
                dwExtraInfo = IntPtr.Zero
            };
            var input = new INPUT { type = INPUT_MOUSE, mi = mi };
            var batch = new[] { input };
            lock (SyntheticMouseLock)
            {
                SendInput(1, batch, InputSizeBytes);
            }
        }

        private async Task HotkeyLoop(CancellationToken token)
        {
            bool leftWasPressed = true; // Initialize to true so it requires a release first
            bool rightWasPressed = true;
            while (!token.IsCancellationRequested)
            {
                if (isBindingHotkey)
                {
                    leftWasPressed = true;
                    rightWasPressed = true;
                }
                else
                {
                    if (leftProfile.Hotkey != 0)
                    {
                        short state = GetAsyncKeyState(leftProfile.Hotkey);
                        bool isPressed = (state & 0x8000) != 0;
                        if (isPressed && !leftWasPressed) ToggleClicker(leftProfile);
                        leftWasPressed = isPressed;
                    }

                    if (rightProfile.Hotkey != 0)
                    {
                        short state = GetAsyncKeyState(rightProfile.Hotkey);
                        bool isPressed = (state & 0x8000) != 0;
                        if (isPressed && !rightWasPressed) ToggleClicker(rightProfile);
                        rightWasPressed = isPressed;
                    }
                }
                await Task.Delay(10, token);
            }
        }

        private void SuspendKeyBtn_Click(object sender, RoutedEventArgs e)
        {
            if (isBindingHotkey) return;
            isBindingHotkey = true;
            SuspendKeyBtn.Content = "Press Key...";
            this.KeyDown += OnSuspendKeyDown;
            this.MouseDown += OnSuspendMouseDown;
            this.Focus();
        }

        private void OnSuspendKeyDown(object sender, KeyEventArgs e)
        {
            this.KeyDown -= OnSuspendKeyDown;
            this.MouseDown -= OnSuspendMouseDown;
            isBindingHotkey = false;

            if (e.Key == Key.Escape)
            {
                currentProfile.SuspendKey = 0;
                SuspendKeyBtn.Content = "None";
            }
            else
            {
                int virtualKey = KeyInterop.VirtualKeyFromKey(e.Key);
                currentProfile.SuspendKey = virtualKey;
                SuspendKeyBtn.Content = e.Key.ToString();
            }
            e.Handled = true;
            SaveConfig();
        }

        private void OnSuspendMouseDown(object sender, MouseButtonEventArgs e)
        {
            this.KeyDown -= OnSuspendKeyDown;
            this.MouseDown -= OnSuspendMouseDown;
            isBindingHotkey = false;

            int vk = MouseButtonToVirtualKey(e.ChangedButton);
            if (vk != 0)
            {
                currentProfile.SuspendKey = vk;
                SuspendKeyBtn.Content = VirtualKeyToString(vk);
            }
            e.Handled = true;
            SaveConfig();
        }

        private void BlockBreakingCheck_Click(object sender, RoutedEventArgs e)
        {
            // Removed
        }

        private void AvoidGuiCheck_Click(object sender, RoutedEventArgs e)
        {
            if (isUpdatingUI) return;
            currentProfile.AvoidGui = AvoidGuiCheck.IsChecked ?? false;
            SaveConfig();
        }

        private void JitterCheck_Click(object sender, RoutedEventArgs e)
        {
            if (isUpdatingUI) return;
            currentProfile.Jitter = JitterCheck.IsChecked ?? false;
            JitterIntensityCard.Visibility = currentProfile.Jitter ? Visibility.Visible : Visibility.Collapsed;
            SaveConfig();
        }

        private void JitterSlider_ValueChanged(object sender, RoutedPropertyChangedEventArgs<double> e)
        {
            if (isUpdatingUI) return;
            if (JitterSlider == null || JitterText == null) return;
            currentProfile.JitterIntensity = (int)JitterSlider.Value;
            JitterText.Text = currentProfile.JitterIntensity.ToString();
            SaveConfig();
        }

        private void PreciseDelay(int delayMs, Func<bool> condition, CancellationToken token)
        {
            Stopwatch sw = Stopwatch.StartNew();
            while (sw.ElapsedMilliseconds < delayMs)
            {
                if (token.IsCancellationRequested || (condition != null && !condition())) break;
                if (delayMs - sw.ElapsedMilliseconds > 15)
                    Thread.Sleep(1);
                else
                    Thread.SpinWait(10);
            }
        }

        private async Task ClickerLoop(ClickerProfile profile, CancellationToken token)
        {
            long btnDownTime = 0;
            bool isHolding = false;
            bool wasClicking = false;
            HumanizedDelayGenerator delayGenerator = new HumanizedDelayGenerator();

            while (!token.IsCancellationRequested)
            {
                bool shouldClick = profile.IsEnabled && IsMinecraftActive();
                if (shouldClick && profile.AvoidGui && IsCursorVisible())
                {
                    shouldClick = false;
                }

                // Check Suspend Key
                if (shouldClick && profile.SuspendKey != 0)
                {
                    short suspendState = GetAsyncKeyState(profile.SuspendKey);
                    if ((suspendState & 0x8000) != 0)
                    {
                        shouldClick = false; // Pause clicking while suspend key is held
                    }
                }

                uint downEvent = profile.IsLeft ? MOUSEEVENTF_LEFTDOWN : MOUSEEVENTF_RIGHTDOWN;
                uint upEvent = profile.IsLeft ? MOUSEEVENTF_LEFTUP : MOUSEEVENTF_RIGHTUP;

                if (shouldClick)
                {
                    bool physicalBtnDown = profile.IsLeft ? physicalLmbDown : physicalRmbDown;

                    if (physicalBtnDown)
                    {
                        wasClicking = true;
                        if (btnDownTime == 0)
                        {
                            btnDownTime = Environment.TickCount64;
                            isHolding = false;
                        }

                        var (upTime, downTime) = delayGenerator.GetDelays(profile.MinCps, profile.MaxCps, comboMode: false);
                        int ut = upTime;
                        int dt = downTime;
                        if (IsForegroundBedrock())
                            ClampBedrockDelays(ref ut, ref dt);

                        SendSyntheticMouse(upEvent);
                        PreciseDelay(ut, () => profile.IsLeft ? physicalLmbDown : physicalRmbDown, token);
                        
                        // Check if user released during the delay
                        bool stillDown = profile.IsLeft ? physicalLmbDown : physicalRmbDown;
                        if (!stillDown)
                        {
                            btnDownTime = 0;
                            continue;
                        }

                        SendSyntheticMouse(downEvent);
                        
                        if (profile.Jitter)
                        {
                            int jx = random.Next(-profile.JitterIntensity, profile.JitterIntensity + 1);
                            int jy = random.Next(-profile.JitterIntensity, profile.JitterIntensity + 1);
                            if (jx != 0 || jy != 0)
                                SendSyntheticMouse(MOUSEEVENTF_MOVE, jx, jy);
                        }

                        PreciseDelay(dt, () => profile.IsLeft ? physicalLmbDown : physicalRmbDown, token);
                    }
                    else
                    {
                        btnDownTime = 0;
                        if (wasClicking || isHolding)
                        {
                            SendSyntheticMouse(upEvent);
                            isHolding = false;
                            wasClicking = false;
                        }
                        await Task.Delay(10, token);
                    }
                }
                else
                {
                    btnDownTime = 0;
                    if (wasClicking || isHolding)
                    {
                        SendSyntheticMouse(upEvent);
                        isHolding = false;
                        wasClicking = false;
                    }
                    await Task.Delay(50, token);
                }
            }
        }

        protected override void OnClosed(EventArgs e)
        {
            UnhookWindowsHookEx(_hookID);
            cts.Cancel();
            base.OnClosed(e);
        }
        private void GithubBtn_Click(object sender, RoutedEventArgs e)
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = "https://github.com/center2055/CitronClicker",
                UseShellExecute = true
            });
        }
    }
}
