// Open Universe — лаунчер (безрамочное окно в стиле Steam/Epic).
//
// Возможности:
//   * собственное окно без рамки Windows: баннер, свои кнопки свернуть/закрыть;
//   * регистрация и вход по логину и паролю (профиль хранится локально);
//   * автоматический вход, если логин и пароль уже вводились;
//   * выбор языка интерфейса (русский / английский) с запоминанием;
//   * показ установленной версии игры и доступной новой;
//   * обновление только когда версии различаются (иначе — «уже актуально»);
//   * новости об обновлениях (тела релизов GitHub прямо в окне);
//   * кнопка «Играть» для запуска установленной игры.
//
// Сборка (Windows, .NET Framework 4.x):
//   csc /target:winexe /out:OpenUniverseLauncher.exe /win32icon:assets\openuniverse.ico
//       /r:System.Windows.Forms.dll /r:System.Drawing.dll
//       /r:System.Web.Extensions.dll /r:System.Net.Http.dll /r:System.Core.dll
//       launcher/Launcher.cs

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows.Forms;

namespace OpenUniverseLauncher
{
    // ---------- Палитра и мелкие помощники рисования ----------
    internal static class Skin
    {
        public static readonly Color Bg = Color.FromArgb(10, 13, 29);
        public static readonly Color Card = Color.FromArgb(18, 23, 48);
        public static readonly Color CardBorder = Color.FromArgb(35, 42, 77);
        public static readonly Color Text = Color.FromArgb(231, 236, 255);
        public static readonly Color Muted = Color.FromArgb(147, 160, 196);
        public static readonly Color Accent = Color.FromArgb(106, 139, 255);
        public static readonly Color Accent2 = Color.FromArgb(0, 230, 208);
        public static readonly Color Field = Color.FromArgb(13, 18, 40);
        public static readonly Color FieldBorder = Color.FromArgb(45, 54, 96);
        public static readonly Color Danger = Color.FromArgb(255, 110, 120);
        public static readonly Color Ok = Color.FromArgb(90, 220, 160);

        public static GraphicsPath Rounded(Rectangle r, int rad)
        {
            var p = new GraphicsPath();
            int d = rad * 2;
            if (d > r.Height) d = r.Height;
            if (d > r.Width) d = r.Width;
            p.AddArc(r.X, r.Y, d, d, 180, 90);
            p.AddArc(r.Right - d, r.Y, d, d, 270, 90);
            p.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
            p.AddArc(r.X, r.Bottom - d, d, d, 90, 90);
            p.CloseFigure();
            return p;
        }
    }

    // ---------- Карточка со скруглёнными углами ----------
    internal sealed class Card : Panel
    {
        public Card()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.OptimizedDoubleBuffer, true);
            BackColor = Skin.Bg;
        }
        protected override void OnPaint(PaintEventArgs e)
        {
            e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            var r = new Rectangle(0, 0, Width - 1, Height - 1);
            using (var path = Skin.Rounded(r, 14))
            using (var fill = new SolidBrush(Skin.Card))
            using (var pen = new Pen(Skin.CardBorder))
            {
                e.Graphics.FillPath(fill, path);
                e.Graphics.DrawPath(pen, path);
            }
        }
    }

    // ---------- Поле ввода со скруглённой рамкой ----------
    internal sealed class Field : Panel
    {
        public readonly TextBox Box;
        private bool _focus;

        public Field(bool password)
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.OptimizedDoubleBuffer, true);
            BackColor = Skin.Card;
            Box = new TextBox
            {
                BorderStyle = BorderStyle.None,
                BackColor = Skin.Field,
                ForeColor = Skin.Text,
                Font = new Font("Segoe UI", 10f),
                UseSystemPasswordChar = password,
                ShortcutsEnabled = true
            };
            Box.GotFocus += (s, e) => { _focus = true; Invalidate(); };
            Box.LostFocus += (s, e) => { _focus = false; Invalidate(); };
            Controls.Add(Box);
            Height = 40;
        }

        protected override void OnResize(EventArgs e)
        {
            base.OnResize(e);
            if (Box != null)
                Box.SetBounds(13, (Height - Box.PreferredHeight) / 2 + 1, Width - 26, Box.PreferredHeight);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            var r = new Rectangle(0, 0, Width - 1, Height - 1);
            using (var path = Skin.Rounded(r, 9))
            using (var fill = new SolidBrush(Skin.Field))
            using (var pen = new Pen(_focus ? Skin.Accent : Skin.FieldBorder, _focus ? 1.6f : 1f))
            {
                e.Graphics.FillPath(fill, path);
                e.Graphics.DrawPath(pen, path);
            }
        }
    }

    // ---------- Кнопка со скруглёнными углами ----------
    internal sealed class FlatBtn : Button
    {
        public bool Primary;
        public bool AccentGradient;
        private bool _hover;

        public FlatBtn()
        {
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.OptimizedDoubleBuffer, true);
            FlatStyle = FlatStyle.Flat;
            BackColor = Skin.Card;
            ForeColor = Skin.Text;
            Font = new Font("Segoe UI", 9.75f, FontStyle.Bold);
            Cursor = Cursors.Hand;
            FlatAppearance.BorderSize = 0;
        }

        protected override void OnMouseEnter(EventArgs e) { _hover = true; Invalidate(); base.OnMouseEnter(e); }
        protected override void OnMouseLeave(EventArgs e) { _hover = false; Invalidate(); base.OnMouseLeave(e); }
        protected override void OnEnabledChanged(EventArgs e) { Invalidate(); base.OnEnabledChanged(e); }

        protected override void OnPaint(PaintEventArgs e)
        {
            e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            var r = new Rectangle(0, 0, Width - 1, Height - 1);
            using (var path = Skin.Rounded(r, 10))
            {
                if (!Enabled)
                {
                    using (var f = new SolidBrush(Color.FromArgb(28, 34, 64)))
                        e.Graphics.FillPath(f, path);
                }
                else if (AccentGradient)
                {
                    using (var f = new LinearGradientBrush(r, Skin.Accent, Skin.Accent2, LinearGradientMode.Horizontal))
                        e.Graphics.FillPath(f, path);
                }
                else if (Primary)
                {
                    using (var f = new SolidBrush(_hover ? Color.FromArgb(120, 150, 255) : Skin.Accent))
                        e.Graphics.FillPath(f, path);
                }
                else
                {
                    Color bg = _hover ? Color.FromArgb(38, 47, 88) : Color.FromArgb(28, 34, 64);
                    using (var f = new SolidBrush(bg)) e.Graphics.FillPath(f, path);
                    using (var pen = new Pen(Skin.CardBorder)) e.Graphics.DrawPath(pen, path);
                }

                Color fore;
                if (AccentGradient) fore = Color.FromArgb(5, 16, 26);
                else if (!Enabled) fore = Color.FromArgb(110, 120, 150);
                else if (Primary) fore = Color.White;
                else fore = _hover ? Color.White : Skin.Text;

                TextRenderer.DrawText(e.Graphics, Text, Font, r, fore,
                    TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix);
            }
        }
    }

    // ---------- Сегментный переключатель (вкладки) ----------
    internal sealed class Segmented : Panel
    {
        public int Selected;
        public event EventHandler Changed;
        private string[] _items;
        private int _hover = -1;

        public Segmented(string[] items)
        {
            _items = items;
            SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.UserPaint | ControlStyles.OptimizedDoubleBuffer, true);
            BackColor = Skin.Card;
            Height = 36;
            Cursor = Cursors.Hand;
        }

        public void SetItems(string[] items)
        {
            _items = items;
            Invalidate();
        }

        private Rectangle ItemRect(int i)
        {
            int w = Width / _items.Length;
            return new Rectangle(i * w, 0, w, Height);
        }

        protected override void OnMouseMove(MouseEventArgs e)
        {
            int h = -1;
            for (int i = 0; i < _items.Length; i++) if (ItemRect(i).Contains(e.Location)) h = i;
            if (h != _hover) { _hover = h; Invalidate(); }
            base.OnMouseMove(e);
        }
        protected override void OnMouseLeave(EventArgs e) { _hover = -1; Invalidate(); base.OnMouseLeave(e); }
        protected override void OnMouseDown(MouseEventArgs e)
        {
            for (int i = 0; i < _items.Length; i++)
                if (ItemRect(i).Contains(e.Location)) { Selected = i; Invalidate(); if (Changed != null) Changed(this, EventArgs.Empty); }
            base.OnMouseDown(e);
        }

        protected override void OnPaint(PaintEventArgs e)
        {
            e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
            using (var path = Skin.Rounded(new Rectangle(0, 0, Width - 1, Height - 1), 10))
            using (var fill = new SolidBrush(Color.FromArgb(12, 16, 36)))
            using (var pen = new Pen(Skin.CardBorder))
            {
                e.Graphics.FillPath(fill, path);
                e.Graphics.DrawPath(pen, path);
            }
            for (int i = 0; i < _items.Length; i++)
            {
                var r = ItemRect(i);
                r.Inflate(-3, -3);
                bool on = i == Selected;
                using (var path = Skin.Rounded(r, 8))
                {
                    if (on)
                        using (var f = new LinearGradientBrush(r, Skin.Accent, Skin.Accent2, LinearGradientMode.Horizontal))
                            e.Graphics.FillPath(f, path);
                    else if (i == _hover)
                        using (var f = new SolidBrush(Color.FromArgb(38, 47, 88)))
                            e.Graphics.FillPath(f, path);

                    Color fore = on ? Color.FromArgb(5, 16, 26) : Skin.Muted;
                    TextRenderer.DrawText(e.Graphics, _items[i], Font, r, fore,
                        TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix);
                }
            }
        }
    }

    internal static class Program
    {
        private static Mutex _single;

        [STAThread]
        private static void Main()
        {
            // Страховка: любые исключения в UI-потоке пишем в лог, чтобы не падать молча.
            AppDomain.CurrentDomain.UnhandledException += (s, e) =>
                LogCrash(e.ExceptionObject as Exception);
            Application.ThreadException += (s, e) =>
                LogCrash(e.Exception);

            try { ServicePointManager.SecurityProtocol |= SecurityProtocolType.Tls12 | SecurityProtocolType.Tls11; }
            catch { }

            bool created;
            _single = new Mutex(true, "OpenUniverseLauncher_SingleInstance", out created);
            if (!created) return; // уже запущен — второе окно не открываем

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new LauncherForm());
            GC.KeepAlive(_single);
        }

        private static void LogCrash(Exception ex)
        {
            try
            {
                string dir = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                    "OpenUniverse");
                Directory.CreateDirectory(dir);
                File.AppendAllText(Path.Combine(dir, "last-error.log"),
                    DateTime.Now.ToString("s") + "  " + (ex == null ? "(null)" : ex.ToString()) + Environment.NewLine);
            }
            catch { }
        }
    }

    internal sealed class PilotProfile
    {
        public string Login;
        public string PasswordHash;
        public bool Remember;
    }

    internal sealed class ReleaseInfo
    {
        public string Tag;
        public string Name;
        public DateTime Date;
        public string Notes;
        public string GameSetupUrl;
        public string GameSetupName;
        public string GameSetupTag;
        public double GameSetupSizeMb;
        public string LauncherUrl;
    }

    internal sealed class LauncherForm : Form
    {
        private const string RepoJson =
            "https://andreyshpak2016-lgtm.github.io/open-universe/releases.json";
        private const string RepoApi =
            "https://api.github.com/repos/andreyshpak2016-lgtm/open-universe/releases?per_page=10";
        private const string StatusUrl =
            "https://andreyshpak2016-lgtm.github.io/open-universe/status.json";
        private const string SiteUrl = "https://andreyshpak2016-lgtm.github.io/open-universe/";

        private const int WM_NCLBUTTONDOWN = 0xA1;
        private const int HTCAPTION = 0x2;

        [DllImport("user32.dll")] private static extern bool ReleaseCapture();
        [DllImport("user32.dll")] private static extern IntPtr SendMessage(IntPtr hWnd, int msg, IntPtr wParam, IntPtr lParam);

        private readonly string _profilePath;
        private readonly string _settingsPath;
        private readonly string _appDir;
        private PilotProfile _profile;
        private readonly List<ReleaseInfo> _releases = new List<ReleaseInfo>();

        private string _lang = "ru";
        private string _installedDisplay;
        private string _latestGameDisplay;

        // Статус «сервер+клиент» с сайта (status.json).
        private bool _healthKnown;
        private bool _healthOk;
        private int _healthTick;
        private double _healthLatencyMs;
        private string _healthNotes = "";

        // ---------------- Локализация ----------------
        private static readonly Dictionary<string, string[]> S = new Dictionary<string, string[]>();
        private static void L(string key, string ru, string en) { S[key] = new string[] { ru, en }; }

        static LauncherForm()
        {
            L("app.title", "Open Universe — лаунчер", "Open Universe — launcher");
            L("tagline", "Один общий космос. Построен игроками.", "One shared universe. Built by players.");

            L("tab.signin", "Вход", "Sign in");
            L("tab.register", "Регистрация", "Register");
            L("auth.title", "Профиль пилота", "Pilot profile");
            L("label.login", "Логин", "Login");
            L("label.password", "Пароль", "Password");
            L("label.callname", "Позывной", "Call sign");
            L("label.passwordMin", "Пароль (от 4 символов)", "Password (min 4)");
            L("label.repeat", "Повторите пароль", "Repeat password");
            L("chk.remember", "Запомнить логин", "Remember login");
            L("chk.rules", "Принимаю правила сообщества", "I accept the community rules");
            L("btn.signin", "Войти", "Sign in");
            L("btn.create", "Создать профиль", "Create profile");

            L("signed.title", "Вы вошли в систему", "You are signed in");
            L("signed.pilot", "Пилот: {0}", "Pilot: {0}");
            L("signed.note", "Логин и пароль сохранены — вход выполняется автоматически.",
                "Login and password are saved — sign-in happens automatically.");

            L("ver.installed", "Установлено", "Installed");
            L("ver.latest", "Доступно", "Available");
            L("ver.none", "не установлена", "not installed");

            L("health.label", "Сервер+клиент", "Server+client");
            L("health.checking", "проверяется…", "checking…");
            L("health.ok", "работают", "working");
            L("health.fail", "проблемы", "issues");
            L("health.unknown", "нет данных", "no data");

            L("news.title", "Новости и обновления", "News and updates");
            L("news.loading", "Загружаю новости…", "Loading news…");
            L("news.noVersions", "Не удалось получить список версий. Проверьте интернет-соединение.",
                "Could not get the version list. Check your internet connection.");
            L("news.fail", "Не удалось загрузить новости. Проверьте интернет-соединение.",
                "Could not load the news. Check your internet connection.");

            L("btn.play", "Играть", "Play");
            L("btn.playNotInstalled", "Играть (не установлена)", "Play (not installed)");
            L("btn.playSignIn", "Играть (войдите)", "Play (sign in)");
            L("btn.update", "Проверить обновления", "Check for updates");
            L("btn.site", "Сайт", "Website");
            L("btn.folder", "Папка игры", "Game folder");
            L("btn.logout", "Выйти", "Sign out");

            L("status.ready", "Готово к работе.", "Ready.");
            L("status.checking", "Проверяю обновления…", "Checking for updates…");
            L("status.upToDate", "Установлена актуальная версия {0} — обновление не требуется.",
                "You already have the latest version {0} — no update needed.");
            L("status.updateAvail", "Доступно обновление: установлено {0}, новая версия {1}. Нажмите «Проверить обновления».",
                "Update available: installed {0}, new version {1}. Press “Check for updates”.");
            L("status.notInstalled", "Игра не установлена. Доступна версия {0}.",
                "The game is not installed. Version {0} is available.");
            L("status.noInstaller", "В релизах нет установщика игры.", "No game installer found in the releases.");
            L("status.noVersions", "Не удалось получить список версий (нет сети?).",
                "Could not get the version list (no network?).");
            L("status.downloading", "Скачиваю… {0:N0} / {1:N0} МБ", "Downloading… {0:N0} / {1:N0} MB");
            L("status.downloading2", "Скачиваю… {0:N0} МБ", "Downloading… {0:N0} MB");
            L("status.installing", "Устанавливаю игру в фоне…", "Installing the game in the background…");
            L("status.installing2", "Идёт установка игры…", "Installing the game…");
            L("status.installed", "Готово: игра установлена ({0}). Нажмите «Играть».",
                "Done: the game is installed ({0}). Press Play.");
            L("status.installFail", "Не удалось установить игру: ", "Could not install the game: ");
            L("status.launching", "Игра запускается…", "Launching the game…");
            L("status.launchFail", "Не удалось запустить игру: ", "Could not launch the game: ");
            L("status.updateError", "Ошибка проверки обновлений: ", "Update check error: ");
            L("status.needSignIn", "Сначала войдите или зарегистрируйтесь.", "Please sign in or register first.");
            L("status.noGameFile", "Игра не найдена. Нажмите «Проверить обновления».",
                "The game was not found. Press “Check for updates”.");

            L("hint.signedIn", "Вы вошли как {0}.", "Signed in as {0}.");
            L("hint.welcome", "Добро пожаловать, {0}!", "Welcome, {0}!");
            L("hint.signedOut", "Вы вышли из профиля.", "You signed out.");
            L("hint.created", "Профиль «{0}» создан. Можно играть.", "Profile “{0}” created. You can play.");

            L("err.callnameShort", "Позывной — минимум 2 символа.", "Call sign must be at least 2 characters.");
            L("err.callnameChars", "В позывном — только буквы, цифры, пробел, точка, дефис.",
                "The call sign may contain only letters, digits, spaces, dots and hyphens.");
            L("err.passShort", "Пароль — минимум 4 символа.", "Password must be at least 4 characters.");
            L("err.passMismatch", "Пароли не совпадают.", "Passwords do not match.");
            L("err.needRules", "Нужно принять правила сообщества.", "You must accept the community rules.");
            L("err.enterLogin", "Введите логин.", "Enter your login.");
            L("err.enterPass", "Введите пароль.", "Enter your password.");
            L("err.profileMissing", "Профиль не найден. Откройте «Регистрация».",
                "Profile not found. Open “Register”.");
            L("err.wrongPass", "Неверный пароль.", "Wrong password.");

            L("prompt.title", "Обновление игры", "Game update");
            L("prompt.download", "Игра не установлена.\r\n\r\nСкачать игру {0} ({1} МБ)?",
                "The game is not installed.\r\n\r\nDownload the game {0} ({1} MB)?");
            L("prompt.update", "Доступно обновление игры {0} (установлена {1}).\r\n\r\nСкачать обновление?",
                "Game update {0} is available (installed {1}).\r\n\r\nDownload the update?");
        }

        private string T(string key)
        {
            string[] pair;
            if (S.TryGetValue(key, out pair)) return _lang == "en" ? pair[1] : pair[0];
            return key;
        }

        // ---------------- Поля интерфейса ----------------
        private Panel _header;
        private Bitmap _banner;
        private readonly Rectangle _rectClose = new Rectangle(952, 14, 32, 30);
        private readonly Rectangle _rectMin = new Rectangle(912, 14, 32, 30);
        private readonly Rectangle _rectLangRu = new Rectangle(796, 14, 52, 28);
        private readonly Rectangle _rectLangEn = new Rectangle(848, 14, 52, 28);
        private int _hoverClose, _hoverMin, _hoverLang;

        private Card _authCard, _newsCard;
        private Segmented _authToggle;
        private Panel _signInPanel, _regPanel, _signedPanel;
        private Field _fLogin, _fPass, _fRegLogin, _fRegPass, _fRegPass2;
        private Label _lblLogin, _lblPassword, _lblCallname, _lblPassMin, _lblRepeat;
        private CheckBox _remember, _regRules;
        private FlatBtn _enterBtn, _regBtn;
        private Label _authHint, _signedTitle, _signedPilot, _signedNote, _authTitle;
        private Label _newsTitle, _status;
        private RichTextBox _news;
        private FlatBtn _play, _update, _site, _folder, _logout;
        private ProgressBar _progress;

        public LauncherForm()
        {
            _appDir = Path.GetDirectoryName(Application.ExecutablePath);
            _profilePath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "OpenUniverse", "launcher.ini");
            _settingsPath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "OpenUniverse", "settings.ini");

            string saved = ReadSetting("lang");
            _lang = (saved == "ru" || saved == "en") ? saved : DetectDefaultLang();

            Text = T("app.title");
            FormBorderStyle = FormBorderStyle.None;
            ClientSize = new Size(1000, 792);
            StartPosition = FormStartPosition.CenterScreen;
            MaximizeBox = false;
            MinimizeBox = true;
            DoubleBuffered = true;
            BackColor = Skin.Bg;
            ForeColor = Skin.Text;
            Font = new Font("Segoe UI", 9.5f);
            TryEnableRoundedCorners();

            BuildUi();
            ApplyLanguage();
            LoadProfile();

            Shown += async (s, e) =>
            {
                await RefreshNewsAsync();
                await CheckForUpdateAsync(silent: true);
                await FetchHealthAsync();
            };
        }

        /// <summary>Прочитать status.json с сайта: статус «сервер+клиент».</summary>
        private async Task FetchHealthAsync()
        {
            try
            {
                string json = await HttpGetAsync(StatusUrl);
                var ser = new JavaScriptSerializer { MaxJsonLength = int.MaxValue };
                var d = ser.Deserialize<Dictionary<string, object>>(json);
                if (d == null)
                {
                    _healthKnown = false;
                }
                else
                {
                    _healthKnown = true;
                    _healthOk = GetBool(d, "ok");
                    object tick;
                    if (d.TryGetValue("tick", out tick) && tick != null)
                        _healthTick = Convert.ToInt32(tick);
                    object lat;
                    if (d.TryGetValue("latencyMs", out lat) && lat != null)
                        _healthLatencyMs = Convert.ToDouble(lat);
                    _healthNotes = GetStr(d, "notes") ?? "";
                }
            }
            catch
            {
                _healthKnown = false;
            }
            _header.Invalidate();
        }

        private static bool GetBool(Dictionary<string, object> d, string key)
        {
            object v;
            if (d.TryGetValue(key, out v) && v != null)
            {
                bool b;
                if (bool.TryParse(v.ToString(), out b)) return b;
            }
            return false;
        }

        private static string GetStr(Dictionary<string, object> d, string key)
        {
            object v;
            if (d.TryGetValue(key, out v) && v != null) return v.ToString();
            return null;
        }

        // Закруглённые углы окна в Windows 11 (на старых ОС просто не сработает).
        private void TryEnableRoundedCorners()
        {
            try
            {
                int pref = 2; // DWMWCP_ROUND
                int size = Marshal.SizeOf(pref);
                IntPtr p = Marshal.AllocHGlobal(size);
                Marshal.WriteInt32(p, pref);
                // 33 = DWMWA_WINDOW_CORNER_PREFERENCE
                DwmSetWindowAttribute(Handle, 33, p, size);
                Marshal.FreeHGlobal(p);
            }
            catch { }
        }

        [DllImport("dwmapi.dll")] private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attr, IntPtr value, int size);

        private static string DetectDefaultLang()
        {
            try
            {
                if (System.Globalization.CultureInfo.CurrentUICulture.TwoLetterISOLanguageName == "ru") return "ru";
            }
            catch { }
            return "en";
        }

        // ---------------- Построение интерфейса ----------------
        private void BuildUi()
        {
            // ---- Шапка с баннером ----
            _header = new Panel { Dock = DockStyle.Top, Height = 236, BackColor = Skin.Bg };
            _header.Paint += HeaderPaint;
            _header.MouseMove += (s, e) => HeaderMouseMove(e.Location);
            _header.MouseLeave += (s, e) => { _hoverClose = _hoverMin = _hoverLang = 0; _header.Cursor = Cursors.Default; _header.Invalidate(); };
            _header.MouseDown += (s, e) => HeaderMouseDown(e);
            Controls.Add(_header);

            string banner = Path.Combine(_appDir, "banner.jpg");
            if (!File.Exists(banner)) banner = Path.Combine(_appDir, "launcher", "banner.jpg");
            if (File.Exists(banner))
            {
                try { _banner = new Bitmap(banner); } catch { _banner = null; }
            }

            // ---- Карточка входа/регистрации ----
            _authCard = new Card { Location = new Point(28, 260), Size = new Size(400, 420) };
            Controls.Add(_authCard);
            BuildAuthCard();

            // ---- Карточка новостей ----
            _newsCard = new Card { Location = new Point(452, 260), Size = new Size(520, 420) };
            Controls.Add(_newsCard);

            _newsTitle = new Label
            {
                Text = "Новости и обновления",
                Font = new Font("Segoe UI", 12.5f, FontStyle.Bold),
                ForeColor = Skin.Text,
                AutoSize = true,
                Location = new Point(20, 16),
                BackColor = Skin.Card
            };
            _newsCard.Controls.Add(_newsTitle);

            _news = new RichTextBox
            {
                Location = new Point(16, 50),
                Size = new Size(488, 350),
                ReadOnly = true,
                BorderStyle = BorderStyle.None,
                BackColor = Skin.Card,
                ForeColor = Color.FromArgb(205, 216, 240),
                ScrollBars = RichTextBoxScrollBars.Vertical,
                DetectUrls = false
            };
            _newsCard.Controls.Add(_news);

            // ---- Нижняя строка статуса ----
            _status = new Label
            {
                Location = new Point(28, 690),
                Size = new Size(620, 22),
                ForeColor = Skin.Muted,
                BackColor = Skin.Bg,
                Text = "Готово к работе.",
                AutoEllipsis = true
            };
            Controls.Add(_status);

            // ---- Кнопки действий ----
            _play = new FlatBtn { Text = "Играть", AccentGradient = true, Location = new Point(28, 718), Size = new Size(220, 46), Font = new Font("Segoe UI", 11f, FontStyle.Bold) };
            _play.Enabled = false;
            _play.Click += OnPlay;
            Controls.Add(_play);

            _update = new FlatBtn { Text = "Проверить обновления", Location = new Point(258, 718), Size = new Size(210, 46) };
            _update.Click += async (s, e) => await CheckForUpdateAsync(silent: false);
            Controls.Add(_update);

            _site = new FlatBtn { Text = "Сайт", Location = new Point(616, 718), Size = new Size(104, 46) };
            _site.Click += (s, e) => Open(SiteUrl);
            Controls.Add(_site);

            _folder = new FlatBtn { Text = "Папка игры", Location = new Point(728, 718), Size = new Size(130, 46) };
            _folder.Click += (s, e) =>
            {
                var dir = GameDir();
                Directory.CreateDirectory(dir);
                Open("explorer.exe", "\"" + dir + "\"");
            };
            Controls.Add(_folder);

            _logout = new FlatBtn { Text = "Выйти", Location = new Point(866, 718), Size = new Size(106, 46) };
            _logout.Click += OnSignOut;
            Controls.Add(_logout);

            _progress = new ProgressBar
            {
                Location = new Point(28, 670),
                Size = new Size(944, 12),
                Style = ProgressBarStyle.Continuous,
                Visible = false
            };
            Controls.Add(_progress);

            // Рамка окна (без системной рамки рисуем свою тонкую).
            Paint += (s, e) =>
            {
                e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
                using (var pen = new Pen(Color.FromArgb(60, 90, 130, 255)))
                    e.Graphics.DrawRectangle(pen, 0, 0, Width - 1, Height - 1);
            };
        }

        private void BuildAuthCard()
        {
            _authTitle = new Label
            {
                Text = "Профиль пилота",
                Font = new Font("Segoe UI", 12.5f, FontStyle.Bold),
                ForeColor = Skin.Text,
                AutoSize = true,
                Location = new Point(20, 16),
                BackColor = Skin.Card
            };
            _authCard.Controls.Add(_authTitle);

            _authToggle = new Segmented(new[] { "Вход", "Регистрация" })
            {
                Location = new Point(20, 52),
                Width = 360
            };
            _authToggle.Changed += (s, e) => ShowAuthTab(_authToggle.Selected);
            _authCard.Controls.Add(_authToggle);

            // ---- Вход ----
            _signInPanel = new Panel { Location = new Point(20, 104), Size = new Size(360, 300), BackColor = Skin.Card };
            _authCard.Controls.Add(_signInPanel);

            _lblLogin = MkFieldLabel("Логин", 0, 0);
            _signInPanel.Controls.Add(_lblLogin);
            _fLogin = new Field(false) { Location = new Point(0, 22), Width = 360 };
            _signInPanel.Controls.Add(_fLogin);

            _lblPassword = MkFieldLabel("Пароль", 0, 78);
            _signInPanel.Controls.Add(_lblPassword);
            _fPass = new Field(true) { Location = new Point(0, 100), Width = 360 };
            _signInPanel.Controls.Add(_fPass);

            _remember = MkCheck("Запомнить логин", 0, 156);
            _signInPanel.Controls.Add(_remember);

            _enterBtn = new FlatBtn { Text = "Войти", Primary = true, Location = new Point(0, 196), Size = new Size(360, 46) };
            _enterBtn.Click += OnSignIn;
            _signInPanel.Controls.Add(_enterBtn);

            // ---- Регистрация ----
            _regPanel = new Panel { Location = new Point(20, 104), Size = new Size(360, 300), BackColor = Skin.Card, Visible = false };
            _authCard.Controls.Add(_regPanel);

            _lblCallname = MkFieldLabel("Позывной", 0, 0);
            _regPanel.Controls.Add(_lblCallname);
            _fRegLogin = new Field(false) { Location = new Point(0, 22), Width = 360 };
            _regPanel.Controls.Add(_fRegLogin);

            _lblPassMin = MkFieldLabel("Пароль (от 4 символов)", 0, 68);
            _regPanel.Controls.Add(_lblPassMin);
            _fRegPass = new Field(true) { Location = new Point(0, 90), Width = 360 };
            _regPanel.Controls.Add(_fRegPass);

            _lblRepeat = MkFieldLabel("Повторите пароль", 0, 136);
            _regPanel.Controls.Add(_lblRepeat);
            _fRegPass2 = new Field(true) { Location = new Point(0, 158), Width = 360 };
            _regPanel.Controls.Add(_fRegPass2);

            _regRules = MkCheck("Принимаю правила сообщества", 0, 208);
            _regPanel.Controls.Add(_regRules);

            _regBtn = new FlatBtn { Text = "Создать профиль", AccentGradient = true, Location = new Point(0, 242), Size = new Size(360, 46) };
            _regBtn.Click += OnRegister;
            _regPanel.Controls.Add(_regBtn);

            // ---- Уже вошли ----
            _signedPanel = new Panel { Location = new Point(20, 104), Size = new Size(360, 300), BackColor = Skin.Card, Visible = false };
            _authCard.Controls.Add(_signedPanel);

            _signedTitle = new Label
            {
                Text = "Вы вошли в систему",
                Font = new Font("Segoe UI", 13f, FontStyle.Bold),
                ForeColor = Skin.Ok,
                AutoSize = true,
                Location = new Point(0, 24),
                BackColor = Skin.Card
            };
            _signedPanel.Controls.Add(_signedTitle);

            _signedPilot = new Label
            {
                AutoSize = true,
                Location = new Point(0, 74),
                ForeColor = Skin.Text,
                Font = new Font("Segoe UI", 12f, FontStyle.Bold),
                BackColor = Skin.Card
            };
            _signedPanel.Controls.Add(_signedPilot);

            _signedNote = new Label
            {
                Size = new Size(340, 70),
                Location = new Point(0, 108),
                ForeColor = Skin.Muted,
                BackColor = Skin.Card
            };
            _signedPanel.Controls.Add(_signedNote);

            _authHint = new Label
            {
                Location = new Point(20, 384),
                Size = new Size(360, 30),
                ForeColor = Skin.Muted,
                BackColor = Skin.Card
            };
            _authCard.Controls.Add(_authHint);
        }

        private static Label MkFieldLabel(string text, int x, int y)
        {
            return new Label
            {
                Text = text,
                Location = new Point(x, y),
                AutoSize = true,
                ForeColor = Skin.Muted,
                BackColor = Skin.Card,
                Font = new Font("Segoe UI", 8.5f)
            };
        }

        private static CheckBox MkCheck(string text, int x, int y)
        {
            return new CheckBox
            {
                Text = text,
                Location = new Point(x, y),
                AutoSize = true,
                ForeColor = Color.FromArgb(190, 202, 232),
                BackColor = Skin.Card,
                FlatStyle = FlatStyle.Flat
            };
        }

        // ---------------- Рисование шапки ----------------
        private void HeaderPaint(object sender, PaintEventArgs e)
        {
            var g = e.Graphics;
            g.SmoothingMode = SmoothingMode.AntiAlias;
            var r = new Rectangle(0, 0, _header.Width, _header.Height);

            if (_banner != null)
            {
                g.InterpolationMode = InterpolationMode.HighQualityBicubic;
                // Вписываем баннер по ширине (как cover), обрезая лишнюю высоту.
                float scale = (float)_header.Width / _banner.Width;
                int dh = (int)(_banner.Height * scale);
                int dy = (int)((_header.Height - dh) * 0.42f);
                g.DrawImage(_banner, new Rectangle(0, dy, _header.Width, dh));
            }
            else
            {
                using (var br = new LinearGradientBrush(r, Color.FromArgb(16, 22, 52), Color.FromArgb(6, 8, 20), LinearGradientMode.Vertical))
                    g.FillRectangle(br, r);
            }

            DrawEmblem(g, 88, 128, 62);

            using (var f = new Font("Segoe UI", 26f, FontStyle.Bold))
                TextRenderer.DrawText(g, "OPEN UNIVERSE", f, new Point(174, 70), Color.White,
                    TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);

            using (var f = new Font("Segoe UI", 10.5f))
                TextRenderer.DrawText(g, T("tagline"), f, new Point(177, 118), Color.FromArgb(205, 216, 245),
                    TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);

            // Версии.
            string inst = string.IsNullOrEmpty(_installedDisplay) ? T("ver.none") : Pretty(_installedDisplay);
            string latest = string.IsNullOrEmpty(_latestGameDisplay) ? "—" : Pretty(_latestGameDisplay);
            using (var f = new Font("Segoe UI", 9f))
            {
                TextRenderer.DrawText(g, T("ver.installed") + ": " + inst, f, new Point(177, 152), Color.FromArgb(200, 212, 240),
                    TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
                bool upToDate = !string.IsNullOrEmpty(_installedDisplay) && !string.IsNullOrEmpty(_latestGameDisplay)
                    && Norm(_installedDisplay) == Norm(_latestGameDisplay);
                Color lc = string.IsNullOrEmpty(_latestGameDisplay) ? Color.FromArgb(200, 212, 240)
                    : (upToDate ? Skin.Ok : Color.FromArgb(255, 200, 90));
                TextRenderer.DrawText(g, T("ver.latest") + ": " + latest, f, new Point(177, 172), lc,
                    TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
            }

            // Статус сервер+клиент (с сайта).
            string healthText;
            Color healthColor;
            if (!_healthKnown)
            {
                healthText = T("health.label") + ": " + T("health.checking");
                healthColor = Color.FromArgb(200, 212, 240);
            }
            else if (_healthOk)
            {
                healthText = T("health.label") + ": " + T("health.ok") +
                    string.Format(" (tick {0}, {1:0} ms)", _healthTick, _healthLatencyMs);
                healthColor = Skin.Ok;
            }
            else
            {
                healthText = T("health.label") + ": " + T("health.fail");
                healthColor = Color.FromArgb(255, 120, 130);
            }
            using (var f = new Font("Segoe UI", 9f))
            {
                TextRenderer.DrawText(g, healthText, f, new Point(177, 192), healthColor,
                    TextFormatFlags.NoPrefix | TextFormatFlags.NoPadding);
            }

            // Кнопки окна.
            DrawWindowBtn(g, _rectMin, _hoverMin, "min");
            DrawWindowBtn(g, _rectClose, _hoverClose, "close");

            // Переключатель языка.
            using (var path = Skin.Rounded(new Rectangle(_rectLangRu.X - 2, _rectLangRu.Y - 2, 108, 32), 16))
            using (var fill = new SolidBrush(Color.FromArgb(120, 10, 14, 32)))
            using (var pen = new Pen(Color.FromArgb(90, 120, 150, 255)))
            {
                g.FillPath(fill, path);
                g.DrawPath(pen, path);
            }
            DrawLangSeg(g, _rectLangRu, "RU", _lang == "ru", _hoverLang == 1);
            DrawLangSeg(g, _rectLangEn, "EN", _lang == "en", _hoverLang == 2);
        }

        private void DrawWindowBtn(Graphics g, Rectangle r, int hover, string kind)
        {
            if (hover > 0)
            {
                using (var path = Skin.Rounded(r, 8))
                using (var fill = new SolidBrush(hover == 2 ? Color.FromArgb(200, 200, 60, 70) : Color.FromArgb(120, 40, 50, 90)))
                    g.FillPath(fill, path);
            }
            Color c = hover > 0 ? Color.White : Color.FromArgb(210, 220, 245);
            using (var pen = new Pen(c, 1.6f))
            {
                if (kind == "close")
                {
                    int m = 10;
                    g.DrawLine(pen, r.Left + m, r.Top + m, r.Right - m, r.Bottom - m);
                    g.DrawLine(pen, r.Right - m, r.Top + m, r.Left + m, r.Bottom - m);
                }
                else
                {
                    int m = 10;
                    g.DrawLine(pen, r.Left + m, r.Top + r.Height / 2, r.Right - m, r.Top + r.Height / 2);
                }
            }
        }

        private void DrawLangSeg(Graphics g, Rectangle r, string text, bool active, bool hover)
        {
            var rr = new Rectangle(r.X, r.Y, r.Width, r.Height);
            using (var path = Skin.Rounded(rr, 14))
            {
                if (active)
                {
                    using (var f = new LinearGradientBrush(rr, Skin.Accent, Skin.Accent2, LinearGradientMode.Horizontal))
                        g.FillPath(f, path);
                }
                else if (hover)
                {
                    using (var f = new SolidBrush(Color.FromArgb(70, 90, 130, 255)))
                        g.FillPath(f, path);
                }
                Color c = active ? Color.FromArgb(5, 16, 26) : Color.FromArgb(200, 212, 240);
                using (var f = new Font("Segoe UI", 9f, FontStyle.Bold))
                    TextRenderer.DrawText(g, text, f, rr, c,
                        TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter | TextFormatFlags.NoPrefix);
            }
        }

        // Мини-эмблема чёрной дыры (кольцо + ядро).
        private static void DrawEmblem(Graphics g, float cx, float cy, float R)
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            var rect = new RectangleF(cx - R * 1.35f, cy - R * 0.62f, R * 2.7f, R * 1.24f);
            var st = g.Save();
            g.TranslateTransform(cx, cy);
            g.RotateTransform(-18);
            var lg = new LinearGradientBrush(
                new PointF(-R * 1.35f, 0), new PointF(R * 1.35f, 0),
                Color.FromArgb(255, 106, 139, 255), Color.FromArgb(255, 0, 230, 208));
            using (var glow = new Pen(Color.FromArgb(70, 100, 160, 255), R * 0.34f))
            using (var pen = new Pen(lg, R * 0.19f))
            using (var hi = new Pen(Color.FromArgb(235, 255, 255, 255), R * 0.06f))
            {
                var local = new RectangleF(-R * 1.35f, -R * 0.62f, R * 2.7f, R * 1.24f);
                g.DrawArc(glow, local, 0, 360);
                g.DrawArc(pen, local, 0, 360);
                g.DrawArc(hi, local, 120, 60);
            }
            g.Restore(st);

            float cr = R * 0.52f;
            using (var brush = new SolidBrush(Color.FromArgb(255, 0, 0, 2)))
                g.FillEllipse(brush, cx - cr, cy - cr, cr * 2, cr * 2);
            using (var pen = new Pen(Color.FromArgb(230, 150, 200, 255), cr * 0.12f))
                g.DrawEllipse(pen, cx - cr, cy - cr, cr * 2, cr * 2);
        }

        private void HeaderMouseMove(Point p)
        {
            _hoverClose = _rectClose.Contains(p) ? 2 : 0;
            _hoverMin = _rectMin.Contains(p) ? 1 : 0;
            _hoverLang = _rectLangRu.Contains(p) ? 1 : (_rectLangEn.Contains(p) ? 2 : 0);
            bool onBtn = _hoverClose > 0 || _hoverMin > 0 || _hoverLang > 0;
            _header.Cursor = onBtn ? Cursors.Hand : Cursors.SizeAll;
            _header.Invalidate();
        }

        private void HeaderMouseDown(MouseEventArgs e)
        {
            if (e.Button != MouseButtons.Left) return;
            if (_rectClose.Contains(e.Location)) { Close(); return; }
            if (_rectMin.Contains(e.Location)) { WindowState = FormWindowState.Minimized; return; }
            if (_rectLangRu.Contains(e.Location)) { SetLang("ru"); return; }
            if (_rectLangEn.Contains(e.Location)) { SetLang("en"); return; }

            // Перетаскивание окна за шапку.
            ReleaseCapture();
            SendMessage(Handle, WM_NCLBUTTONDOWN, (IntPtr)HTCAPTION, IntPtr.Zero);
        }

        private void SetLang(string lang)
        {
            if (_lang == lang) return;
            _lang = lang;
            WriteSetting("lang", _lang);
            ApplyLanguage();
            _header.Invalidate();
        }

        // ---------------- Показ вкладок/панелей ----------------
        private void ShowAuthTab(int idx)
        {
            _signInPanel.Visible = idx == 0;
            _regPanel.Visible = idx == 1;
            SetHint("", false);
        }

        private void ShowSignedIn()
        {
            _authToggle.Visible = false;
            _signInPanel.Visible = false;
            _regPanel.Visible = false;
            _signedPanel.Visible = true;
            _signedPilot.Text = string.Format(T("signed.pilot"), _profile.Login);
        }

        private void ShowSignedOut()
        {
            _authToggle.Visible = true;
            _signedPanel.Visible = false;
            ShowAuthTab(_authToggle.Selected);
        }

        // ---------------- Язык ----------------
        private void ApplyLanguage()
        {
            Text = T("app.title");
            if (_authToggle != null)
            {
                _authToggle.SetItems(new[] { T("tab.signin"), T("tab.register") });
                _authToggle.Invalidate();
            }

            _lblLogin.Text = T("label.login");
            _lblPassword.Text = T("label.password");
            _lblCallname.Text = T("label.callname");
            _lblPassMin.Text = T("label.passwordMin");
            _lblRepeat.Text = T("label.repeat");
            _remember.Text = T("chk.remember");
            _regRules.Text = T("chk.rules");
            _enterBtn.Text = T("btn.signin");
            _regBtn.Text = T("btn.create");
            _signedTitle.Text = T("signed.title");
            _signedNote.Text = T("signed.note");
            if (_authTitle != null) _authTitle.Text = T("auth.title");
            _newsTitle.Text = T("news.title");
            _update.Text = T("btn.update");
            _site.Text = T("btn.site");
            _folder.Text = T("btn.folder");
            _logout.Text = T("btn.logout");

            if (string.IsNullOrEmpty(_status.Text) || _status.Text == "Готово к работе." || _status.Text == "Ready.")
                _status.Text = T("status.ready");

            UpdatePlayButton();
            if (_signedPanel.Visible && _profile != null)
                _signedPilot.Text = string.Format(T("signed.pilot"), _profile.Login);
            _header.Invalidate();
        }

        // ---------------- Профиль ----------------
        private void LoadProfile()
        {
            _profile = ReadProfile();
            if (_profile != null && !string.IsNullOrEmpty(_profile.PasswordHash))
            {
                _fLogin.Box.Text = _profile.Login;
                _remember.Checked = _profile.Remember;
                ShowSignedIn();
                SetHint(string.Format(T("hint.signedIn"), _profile.Login), false);
            }
            else
            {
                _profile = null;
                ShowSignedOut();
            }
            UpdatePlayButton();
        }

        private PilotProfile ReadProfile()
        {
            try
            {
                if (!File.Exists(_profilePath)) return null;
                var p = new PilotProfile();
                foreach (var line in File.ReadAllLines(_profilePath))
                {
                    int i = line.IndexOf('=');
                    if (i <= 0) continue;
                    string k = line.Substring(0, i), v = line.Substring(i + 1);
                    if (k == "login") p.Login = v;
                    else if (k == "hash") p.PasswordHash = v;
                    else if (k == "remember") p.Remember = v == "1";
                }
                return string.IsNullOrEmpty(p.Login) ? null : p;
            }
            catch { return null; }
        }

        private void SaveProfile(PilotProfile p)
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(_profilePath));
                File.WriteAllLines(_profilePath, new[]
                {
                    "login=" + p.Login,
                    "hash=" + p.PasswordHash,
                    "remember=" + (p.Remember ? "1" : "0")
                });
            }
            catch { }
        }

        private string ReadSetting(string key)
        {
            try
            {
                if (!File.Exists(_settingsPath)) return null;
                foreach (var line in File.ReadAllLines(_settingsPath))
                {
                    int i = line.IndexOf('=');
                    if (i <= 0) continue;
                    if (line.Substring(0, i) == key) return line.Substring(i + 1);
                }
            }
            catch { }
            return null;
        }

        private void WriteSetting(string key, string value)
        {
            try
            {
                Directory.CreateDirectory(Path.GetDirectoryName(_settingsPath));
                var dict = new Dictionary<string, string>();
                if (File.Exists(_settingsPath))
                    foreach (var line in File.ReadAllLines(_settingsPath))
                    {
                        int i = line.IndexOf('=');
                        if (i > 0) dict[line.Substring(0, i)] = line.Substring(i + 1);
                    }
                dict[key] = value;
                var lines = new List<string>();
                foreach (var kv in dict) lines.Add(kv.Key + "=" + kv.Value);
                File.WriteAllLines(_settingsPath, lines.ToArray());
            }
            catch { }
        }

        private static string Hash(string password)
        {
            using (var sha = SHA256.Create())
            {
                var bytes = sha.ComputeHash(Encoding.UTF8.GetBytes("ou:" + password));
                var sb = new StringBuilder();
                foreach (var b in bytes) sb.Append(b.ToString("x2"));
                return sb.ToString();
            }
        }

        private void OnRegister(object sender, EventArgs e)
        {
            string login = (_fRegLogin.Box.Text ?? "").Trim();
            string p1 = _fRegPass.Box.Text ?? "";
            string p2 = _fRegPass2.Box.Text ?? "";
            if (login.Length < 2) { SetHint(T("err.callnameShort"), true); return; }
            if (!System.Text.RegularExpressions.Regex.IsMatch(login, "^[A-Za-z0-9_.\\-А-Яа-яЁё ]+$"))
            { SetHint(T("err.callnameChars"), true); return; }
            if (p1.Length < 4) { SetHint(T("err.passShort"), true); return; }
            if (p1 != p2) { SetHint(T("err.passMismatch"), true); return; }
            if (!_regRules.Checked) { SetHint(T("err.needRules"), true); return; }

            _profile = new PilotProfile { Login = login, PasswordHash = Hash(p1), Remember = true };
            SaveProfile(_profile);
            SetHint(string.Format(T("hint.created"), login), false);
            ShowSignedIn();
            UpdatePlayButton();
        }

        private void OnSignIn(object sender, EventArgs e)
        {
            string login = (_fLogin.Box.Text ?? "").Trim();
            string pass = _fPass.Box.Text ?? "";
            if (login.Length < 2) { SetHint(T("err.enterLogin"), true); return; }
            if (pass.Length < 1) { SetHint(T("err.enterPass"), true); return; }

            var stored = ReadProfile();
            if (stored == null || !string.Equals(stored.Login, login, StringComparison.OrdinalIgnoreCase))
            {
                SetHint(T("err.profileMissing"), true);
                return;
            }
            if (!string.Equals(stored.PasswordHash, Hash(pass), StringComparison.Ordinal))
            {
                SetHint(T("err.wrongPass"), true);
                return;
            }

            stored.Remember = _remember.Checked;
            _profile = stored;
            SaveProfile(stored);
            SetHint(string.Format(T("hint.welcome"), stored.Login), false);
            ShowSignedIn();
            UpdatePlayButton();
        }

        private void OnSignOut(object sender, EventArgs e)
        {
            _profile = null;
            try { if (File.Exists(_profilePath)) File.Delete(_profilePath); } catch { }
            _fPass.Box.Text = "";
            ShowSignedOut();
            SetHint(T("hint.signedOut"), false);
            UpdatePlayButton();
        }

        private void SetHint(string text, bool error)
        {
            _authHint.Text = text;
            _authHint.ForeColor = error ? Skin.Danger : Skin.Ok;
        }

        // ---------------- Новости ----------------
        private async Task RefreshNewsAsync()
        {
            _news.Text = T("news.loading");
            try
            {
                _releases.Clear();
                _releases.AddRange(await LoadReleasesAsync());

                if (_releases.Count == 0)
                {
                    _news.Text = T("news.noVersions");
                    return;
                }

                var sb = new StringBuilder();
                int shown = 0;
                foreach (var r in _releases)
                {
                    sb.AppendLine("● " + r.Tag + "    " + (r.Date == DateTime.MinValue ? "" : r.Date.ToString("dd.MM.yyyy")));
                    if (!string.IsNullOrWhiteSpace(r.Notes))
                    {
                        var lines = r.Notes.Trim().Split('\n');
                        int printed = 0;
                        foreach (var raw in lines)
                        {
                            string line = raw.Trim();
                            if (line.Length == 0) continue;
                            if (line.StartsWith("#")) line = line.TrimStart('#', ' ');
                            if (line.Length > 200) line = line.Substring(0, 200) + "…";
                            sb.AppendLine("   " + line);
                            if (++printed >= 6) break;
                        }
                    }
                    sb.AppendLine();
                    if (++shown >= 4) break;
                }
                _news.Text = sb.ToString();
            }
            catch (Exception ex)
            {
                _news.Text = T("news.fail") + "\r\n\r\n(" + ex.Message + ")";
            }
        }

        // ---------------- Обновления ----------------
        private async Task CheckForUpdateAsync(bool silent)
        {
            if (!silent) SetStatus(T("status.checking"), false);
            try
            {
                if (_releases.Count == 0)
                    _releases.AddRange(await LoadReleasesAsync());
                if (_releases.Count == 0)
                {
                    _installedDisplay = InstalledVersion();
                    _header.Invalidate();
                    if (!silent) SetStatus(T("status.noVersions"), true);
                    return;
                }

                var latest = _releases[0];
                var installable = FindInstallable(_releases);

                string gameTag = installable == null
                    ? latest.Tag
                    : (string.IsNullOrEmpty(installable.GameSetupTag) ? installable.Tag : installable.GameSetupTag);

                _installedDisplay = InstalledVersion();
                _latestGameDisplay = gameTag;
                _header.Invalidate();

                if (installable == null)
                {
                    if (!silent) SetStatus(T("status.noInstaller"), true);
                    UpdatePlayButton();
                    return;
                }

                bool upToDate = !string.IsNullOrEmpty(_installedDisplay)
                    && Norm(_installedDisplay) == Norm(gameTag);

                if (upToDate)
                {
                    if (!silent) SetStatus(string.Format(T("status.upToDate"), Pretty(gameTag)), false);
                    UpdatePlayButton();
                    return;
                }

                if (silent)
                {
                    if (string.IsNullOrEmpty(_installedDisplay))
                        SetStatus(string.Format(T("status.notInstalled"), Pretty(gameTag)), false);
                    else
                        SetStatus(string.Format(T("status.updateAvail"),
                            Pretty(_installedDisplay), Pretty(gameTag)), false);
                    UpdatePlayButton();
                    return;
                }

                string what;
                if (string.IsNullOrEmpty(_installedDisplay))
                    what = string.Format(T("prompt.download"), Pretty(gameTag), installable.GameSetupSizeMb.ToString("0"));
                else
                    what = string.Format(T("prompt.update"), Pretty(gameTag), Pretty(_installedDisplay));

                if (MessageBox.Show(this, what, T("prompt.title"),
                        MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes)
                {
                    UpdatePlayButton();
                    return;
                }

                await DownloadAndRunInstallerAsync(installable);
            }
            catch (Exception ex)
            {
                if (!silent) SetStatus(T("status.updateError") + ex.Message, true);
            }
        }

        private async Task DownloadAndRunInstallerAsync(ReleaseInfo rel)
        {
            string name = string.IsNullOrEmpty(rel.GameSetupName)
                ? "OpenUniverse-Game-Setup.exe" : rel.GameSetupName;
            rel.GameSetupName = name;
            string file = Path.Combine(Path.GetTempPath(), name);
            _progress.Visible = true;
            _progress.Value = 0;

            try
            {
                using (var client = new WebClient())
                {
                    client.DownloadProgressChanged += (s, e) =>
                    {
                        _progress.Value = Math.Min(100, e.ProgressPercentage);
                        if (e.TotalBytesToReceive > 0)
                            SetStatus(string.Format(T("status.downloading"),
                                e.BytesReceived / 1048576.0, e.TotalBytesToReceive / 1048576.0), false);
                        else
                            SetStatus(string.Format(T("status.downloading2"), e.BytesReceived / 1048576.0), false);
                    };
                    var tcs = new TaskCompletionSource<bool>();
                    client.DownloadFileCompleted += (s, e) =>
                    {
                        if (e.Error != null) tcs.TrySetException(e.Error);
                        else if (e.Cancelled) tcs.TrySetCanceled();
                        else tcs.TrySetResult(true);
                    };
                    client.DownloadFileAsync(new Uri(rel.GameSetupUrl), file);
                    await tcs.Task;
                }

                SetStatus(T("status.installing"), false);
                var psi = new ProcessStartInfo
                {
                    FileName = file,
                    Arguments = "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOCANCEL /SP-",
                    UseShellExecute = false,
                    CreateNoWindow = true
                };
                var inst = Process.Start(psi);
                if (inst != null)
                {
                    SetStatus(T("status.installing2"), false);
                    await Task.Run(() => inst.WaitForExit());
                }

                _installedDisplay = InstalledVersion();
                _header.Invalidate();
                SetStatus(string.Format(T("status.installed"), Pretty(rel.GameSetupTag)), false);
                UpdatePlayButton();
            }
            catch (Exception ex)
            {
                SetStatus(T("status.installFail") + ex.Message, true);
            }
            finally
            {
                _progress.Visible = false;
            }
        }

        private static string Norm(string tag)
        {
            if (string.IsNullOrEmpty(tag)) return "";
            return tag.Trim().TrimStart('v', 'V');
        }

        private static string Pretty(string tag)
        {
            if (string.IsNullOrEmpty(tag)) return "—";
            if (tag[0] == 'v' || tag[0] == 'V') return tag;
            return "v" + tag;
        }

        // ---------------- Запуск игры ----------------
        private static string GameDir()
        {
            return Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "Programs", "Open Universe");
        }

        private static string GameExe()
        {
            string[] candidates =
            {
                Path.Combine(GameDir(), "OpenUniverse.exe"),
                Path.Combine(GameDir(), "OpenUniverse", "Binaries", "Win64", "OpenUniverse-Win64-Shipping.exe")
            };
            foreach (var c in candidates) if (File.Exists(c)) return c;
            return null;
        }

        private static string InstalledVersion()
        {
            try
            {
                string v = Path.Combine(GameDir(), "VERSION");
                if (File.Exists(v)) return File.ReadAllText(v).Trim();
            }
            catch { }
            return null;
        }

        private void UpdatePlayButton()
        {
            bool hasGame = GameExe() != null;
            bool signed = _profile != null;
            _play.Enabled = hasGame && signed;
            if (!hasGame) _play.Text = T("btn.playNotInstalled");
            else if (!signed) _play.Text = T("btn.playSignIn");
            else _play.Text = T("btn.play");
        }

        private void OnPlay(object sender, EventArgs e)
        {
            if (_profile == null) { SetStatus(T("status.needSignIn"), true); return; }
            string exe = GameExe();
            if (exe == null) { SetStatus(T("status.noGameFile"), true); return; }
            try
            {
                // Передаём позывной из лаунчера в игру: на старте клиент сразу
                // знает имя пилота (используется значением -OUName).
                var psi = new ProcessStartInfo
                {
                    FileName = exe,
                    WorkingDirectory = Path.GetDirectoryName(exe),
                    Arguments = _profile != null && !string.IsNullOrEmpty(_profile.Login)
                        ? "-OUName=" + _profile.Login : ""
                };
                Process.Start(psi);
                SetStatus(T("status.launching"), false);
            }
            catch (Exception ex)
            {
                SetStatus(T("status.launchFail") + ex.Message, true);
            }
        }

        private void SetStatus(string text, bool error)
        {
            _status.Text = text;
            _status.ForeColor = error ? Skin.Danger : Skin.Muted;
        }

        private static void Open(string file, string args = null)
        {
            try
            {
                if (args == null) Process.Start(file);
                else Process.Start(file, args);
            }
            catch { }
        }

        // ---------------- Сеть и разбор ----------------
        private async Task<List<ReleaseInfo>> LoadReleasesAsync()
        {
            var list = new List<ReleaseInfo>();
            try
            {
                string json = await HttpGetAsync(RepoJson);
                list = ParseSiteJson(json);
                if (list.Count > 0) return list;
            }
            catch { }

            try
            {
                string json = await HttpGetAsync(RepoApi);
                list = ParseApiJson(json);
            }
            catch { }
            return list;
        }

        private static async Task<string> HttpGetAsync(string url)
        {
            using (var client = new HttpClient())
            {
                client.DefaultRequestHeaders.UserAgent.ParseAdd("OpenUniverseLauncher/1.4");
                return await client.GetStringAsync(url);
            }
        }

        private static List<ReleaseInfo> ParseSiteJson(string json)
        {
            var list = new List<ReleaseInfo>();
            var ser = new JavaScriptSerializer { MaxJsonLength = int.MaxValue };
            var root = ser.Deserialize<Dictionary<string, object>>(json);
            object relObj;
            if (root == null || !root.TryGetValue("releases", out relObj)) return list;
            var arr = relObj as object[];
            if (arr == null) return list;

            foreach (var item in arr)
            {
                var d = item as Dictionary<string, object>;
                if (d == null) continue;
                var info = new ReleaseInfo
                {
                    Tag = Str(d, "tag"),
                    Name = Str(d, "name"),
                    Notes = FirstNonEmpty(Str(d, "notes"), Str(d, "name")),
                    GameSetupUrl = Str(d, "gameSetup"),
                    GameSetupTag = Str(d, "gameSetupTag"),
                    LauncherUrl = Str(d, "launcher")
                };
                DateTime dt;
                if (DateTime.TryParse(Str(d, "date"), out dt)) info.Date = dt;
                object size;
                if (d.TryGetValue("size", out size) && size != null)
                    info.GameSetupSizeMb = Convert.ToDouble(size) / 1048576.0;
                list.Add(info);
            }
            return list;
        }

        private static List<ReleaseInfo> ParseApiJson(string json)
        {
            var list = new List<ReleaseInfo>();
            var ser = new JavaScriptSerializer { MaxJsonLength = int.MaxValue };
            var root = ser.Deserialize<object[]>(json);
            if (root == null) return list;
            foreach (var item in root)
            {
                var rel = item as Dictionary<string, object>;
                if (rel == null) continue;
                var info = new ReleaseInfo
                {
                    Tag = Str(rel, "tag_name"),
                    Name = Str(rel, "name"),
                    Notes = Str(rel, "body")
                };
                DateTime d;
                if (DateTime.TryParse(Str(rel, "published_at"), out d)) info.Date = d;

                var assets = rel.ContainsKey("assets") ? rel["assets"] as object[] : null;
                if (assets != null)
                {
                    foreach (var a in assets)
                    {
                        var asset = a as Dictionary<string, object>;
                        if (asset == null) continue;
                        string name = Str(asset, "name");
                        if (name == null) continue;
                        if (name.StartsWith("OpenUniverse-Game-Setup", StringComparison.Ordinal)
                            && name.EndsWith(".exe", StringComparison.OrdinalIgnoreCase))
                        {
                            info.GameSetupUrl = Str(asset, "browser_download_url");
                            object size;
                            if (asset.TryGetValue("size", out size) && size != null)
                                info.GameSetupSizeMb = Convert.ToDouble(size) / 1048576.0;
                        }
                        else if (name.Equals("OpenUniverseLauncher.exe", StringComparison.OrdinalIgnoreCase))
                        {
                            info.LauncherUrl = Str(asset, "browser_download_url");
                        }
                    }
                }
                list.Add(info);
            }
            return list;
        }

        private static ReleaseInfo FindInstallable(List<ReleaseInfo> rels)
        {
            foreach (var r in rels)
                if (!string.IsNullOrEmpty(r.GameSetupUrl)) return r;
            return null;
        }

        private static string Str(Dictionary<string, object> d, string key)
        {
            object v;
            if (d.TryGetValue(key, out v) && v != null) return v.ToString();
            return null;
        }

        private static string FirstNonEmpty(string a, string b)
        {
            if (!string.IsNullOrWhiteSpace(a)) return a;
            return b;
        }
    }
}
