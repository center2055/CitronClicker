using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;

namespace CitronClicker
{
    public class ClickerProfile
    {
        public bool IsLeft { get; set; }
        public bool IsEnabled { get; set; }
        public int MinCps { get; set; } = 10;
        public int MaxCps { get; set; } = 14;
        public bool AllowBlockBreaking { get; set; } = true;
        public bool AvoidGui { get; set; } = true;
        public int Hotkey { get; set; } = 0;
    }

    public partial class MainWindow : Window
    {
        private const int WH_MOUSE_LL = 14;
        private const int WM_LBUTTONDOWN = 0x0201;
        private const int WM_LBUTTONUP = 0x0202;
        private const int WM_MOUSEMOVE = 0x0200;
        private const uint LLMHF_INJECTED = 0x00000001;

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
        private static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, int dwExtraInfo);

        [DllImport("user32.dll")]
        private static extern IntPtr GetForegroundWindow();

        [DllImport("user32.dll", SetLastError = true)]
        private static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

        private const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
        private const uint MOUSEEVENTF_LEFTUP = 0x0004;
        private const int VK_LBUTTON = 0x01;

        private bool isBindingHotkey = false;

        private CancellationTokenSource cts;
        private Random random = new Random();

        private DispatcherTimer uiTimer;
        private bool isMinecraftRunning = false;

        public MainWindow()
        {
            currentProfile = leftProfile;
            
            InitializeComponent();
            _proc = HookCallback;
            _hookID = SetHook(_proc);

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

        private void UpdateUIFromProfile()
        {
            isUpdatingUI = true;
            MinCpsSlider.Value = currentProfile.MinCps;
            MaxCpsSlider.Value = currentProfile.MaxCps;
            MinCpsText.Text = currentProfile.MinCps.ToString();
            MaxCpsText.Text = currentProfile.MaxCps.ToString();
            BlockBreakingCheck.IsChecked = currentProfile.AllowBlockBreaking;
            AvoidGuiCheck.IsChecked = currentProfile.AvoidGui;
            MasterToggleCheck.IsChecked = currentProfile.IsEnabled;

            if (currentProfile.IsLeft)
            {
                BlockBreakingCard.Visibility = Visibility.Visible;
            }
            else
            {
                BlockBreakingCard.Visibility = Visibility.Hidden;
            }

            if (currentProfile.Hotkey == 0) HotkeyBtn.Content = "None";
            else HotkeyBtn.Content = ((Key)KeyInterop.KeyFromVirtualKey(currentProfile.Hotkey)).ToString();

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

        private void UiTimer_Tick(object sender, EventArgs e)
        {
            bool found = false;
            Process[] procs = Process.GetProcessesByName("Minecraft.Windows");
            if (procs.Length > 0) found = true;
            else
            {
                Process[] javaw = Process.GetProcessesByName("javaw");
                foreach (var p in javaw)
                {
                    if (p.MainWindowTitle.ToLower().Contains("minecraft"))
                    {
                        found = true;
                        break;
                    }
                }
            }

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
                StatusText.Foreground = new SolidColorBrush(Color.FromRgb(228, 242, 101)); // #e4f265
                StatusDot.Fill = new SolidColorBrush(Color.FromRgb(228, 242, 101));
            }
        }

        private IntPtr SetHook(LowLevelMouseProc proc)
        {
            using (Process curProcess = Process.GetCurrentProcess())
            using (ProcessModule curModule = curProcess.MainModule)
            {
                return SetWindowsHookEx(WH_MOUSE_LL, proc, GetModuleHandle(curModule.ModuleName), 0);
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
            // Optional: Save to config file here if needed.
            // For now, we just close the window or show a message.
            MessageBox.Show("Configuration Saved!", "Citron Clicker", MessageBoxButton.OK, MessageBoxImage.Information);
        }

        private void HotkeyBtn_Click(object sender, RoutedEventArgs e)
        {
            isBindingHotkey = true;
            HotkeyBtn.Content = "...";
        }

        protected override void OnKeyDown(KeyEventArgs e)
        {
            base.OnKeyDown(e);
            if (isBindingHotkey)
            {
                if (e.Key == Key.Escape)
                {
                    currentProfile.Hotkey = 0;
                    HotkeyBtn.Content = "None";
                }
                else
                {
                    currentProfile.Hotkey = KeyInterop.VirtualKeyFromKey(e.Key);
                    HotkeyBtn.Content = e.Key.ToString();
                }
                isBindingHotkey = false;
            }
        }

        private bool IsMinecraftActive()
        {
            IntPtr hWnd = GetForegroundWindow();
            if (hWnd == IntPtr.Zero) return false;

            StringBuilder sb = new StringBuilder(256);
            GetWindowText(hWnd, sb, 256);
            string title = sb.ToString();
            if (title == "Minecraft" || title.Contains("Minecraft"))
                return true;

            GetWindowThreadProcessId(hWnd, out uint pid);
            try
            {
                Process proc = Process.GetProcessById((int)pid);
                if (proc.ProcessName.Contains("Minecraft.Windows"))
                    return true;
            }
            catch { }

            return false;
        }

        private async Task HotkeyLoop(CancellationToken token)
        {
            bool leftWasPressed = false;
            bool rightWasPressed = false;
            while (!token.IsCancellationRequested)
            {
                if (!isBindingHotkey)
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

        private void BlockBreakingCheck_Click(object sender, RoutedEventArgs e)
        {
            if (isUpdatingUI) return;
            currentProfile.AllowBlockBreaking = BlockBreakingCheck.IsChecked ?? false;
        }

        private void AvoidGuiCheck_Click(object sender, RoutedEventArgs e)
        {
            if (isUpdatingUI) return;
            currentProfile.AvoidGui = AvoidGuiCheck.IsChecked ?? false;
        }

        private void PreciseDelay(int delayMs, CancellationToken token)
        {
            Stopwatch sw = Stopwatch.StartNew();
            while (sw.ElapsedMilliseconds < delayMs)
            {
                if (token.IsCancellationRequested) break;
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

            while (!token.IsCancellationRequested)
            {
                bool shouldClick = profile.IsEnabled && IsMinecraftActive();
                if (shouldClick && profile.AvoidGui && IsCursorVisible())
                {
                    shouldClick = false;
                }

                if (shouldClick)
                {
                    bool physicalBtnDown = profile.IsLeft ? physicalLmbDown : physicalRmbDown;
                    uint downEvent = profile.IsLeft ? MOUSEEVENTF_LEFTDOWN : MOUSEEVENTF_RIGHTDOWN;
                    uint upEvent = profile.IsLeft ? MOUSEEVENTF_LEFTUP : MOUSEEVENTF_RIGHTUP;

                    if (physicalBtnDown)
                    {
                        if (btnDownTime == 0)
                        {
                            btnDownTime = Environment.TickCount64;
                            isHolding = false;
                        }

                        bool isMouseStill = (Environment.TickCount64 - lastPhysicalMouseMoveTime) > 300;

                        if (profile.AllowBlockBreaking && isMouseStill && profile.IsLeft)
                        {
                            if (!isHolding)
                            {
                                mouse_event(downEvent, 0, 0, 0, 0);
                                isHolding = true;
                            }
                            await Task.Delay(10, token);
                        }
                        else if (!isHolding)
                        {
                            int currentCps = random.Next(profile.MinCps, profile.MaxCps + 1);
                            int totalCycleTime = 1000 / currentCps;
                            int upTime = random.Next(10, Math.Min(30, totalCycleTime / 2));
                            int downTime = totalCycleTime - upTime;

                            mouse_event(upEvent, 0, 0, 0, 0);
                            PreciseDelay(upTime, token);
                            
                            // Check if user released during the delay
                            bool stillDown = profile.IsLeft ? physicalLmbDown : physicalRmbDown;
                            if (!stillDown)
                            {
                                btnDownTime = 0;
                                continue;
                            }

                            mouse_event(downEvent, 0, 0, 0, 0);
                            PreciseDelay(downTime, token);
                        }
                        else
                        {
                            // If we were holding, but mouse started moving again, break the hold and resume clicking
                            if (!isMouseStill)
                            {
                                mouse_event(upEvent, 0, 0, 0, 0);
                                isHolding = false;
                                btnDownTime = Environment.TickCount64; // Reset hold timer
                            }
                            await Task.Delay(10, token);
                        }
                    }
                    else
                    {
                        btnDownTime = 0;
                        if (isHolding)
                        {
                            mouse_event(upEvent, 0, 0, 0, 0);
                            isHolding = false;
                        }
                        await Task.Delay(10, token);
                    }
                }
                else
                {
                    btnDownTime = 0;
                    if (isHolding)
                    {
                        uint upEvent = profile.IsLeft ? MOUSEEVENTF_LEFTUP : MOUSEEVENTF_RIGHTUP;
                        mouse_event(upEvent, 0, 0, 0, 0);
                        isHolding = false;
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
                FileName = "https://github.com/center2055",
                UseShellExecute = true
            });
        }
    }
}