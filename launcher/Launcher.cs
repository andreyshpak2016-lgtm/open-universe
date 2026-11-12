// Open Universe — лаунчер.
//
// Возможности:
//   * регистрация и вход по логину и паролю (профиль хранится локально);
//   * автоматический вход, если логин и пароль уже вводились;
//   * выбор языка интерфейса (русский / английский) с запоминанием;
//   * показ установленной версии игры и доступной новой;
//   * обновление только когда версии различаются (иначе — «уже актуально»);
//   * новости об обновлениях (тела релизов GitHub прямо в окне);
//   * кнопка «Играть» для запуска установленной игры.
//
// Сборка (Windows, .NET Framework 4.x):
//   csc /target:winexe /out:OpenUniverseLauncher.exe
//       /r:System.Windows.Forms.dll /r:System.Drawing.dll
//       /r:System.Web.Extensions.dll /r:System.Net.Http.dll /r:System.Core.dll
//       launcher/Launcher.cs

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows.Forms;

namespace OpenUniverseLauncher
{
    internal static class Program
    {
        // Мьютекс: не даём открыть второй лаунчер (иначе «лаунчер в лаунчере»).
        private static Mutex _single;

        [STAThread]
        private static void Main()
        {
            // GitHub требует TLS 1.2+. Без этой строки .NET Framework не соединится.
            try
            {
                ServicePointManager.SecurityProtocol |= SecurityProtocolType.Tls12 | SecurityProtocolType.Tls11;
            }
            catch { }

            bool created;
            _single = new Mutex(true, "OpenUniverseLauncher_SingleInstance", out created);
            if (!created)
            {
                // Лаунчер уже запущен — второе окно не открываем, выходим молча.
                return;
            }

            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new LauncherForm());
            GC.KeepAlive(_single);
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
        // Основной источник — статический releases.json на сайте (без лимитов API).
        // Резерв — GitHub API (ограничен 60 запросами в час на IP).
        private const string RepoJson =
            "https://andreyshpak2016-lgtm.github.io/open-universe/releases.json";
        private const string RepoApi =
            "https://api.github.com/repos/andreyshpak2016-lgtm/open-universe/releases?per_page=10";
        private const string SiteUrl = "https://andreyshpak2016-lgtm.github.io/open-universe/";

        private readonly string _profilePath;
        private readonly string _settingsPath;
        private PilotProfile _profile;
        private readonly List<ReleaseInfo> _releases = new List<ReleaseInfo>();

        // Язык интерфейса: "ru" или "en".
        private string _lang = "ru";

        // Установленная и доступная версии игры.
        private string _installedDisplay;
        private string _latestGameDisplay;

        // ---------------- Локализация ----------------
        private static readonly Dictionary<string, string[]> S = new Dictionary<string, string[]>();
        private static void Add(string key, string ru, string en) { S[key] = new string[] { ru, en }; }

        static LauncherForm()
        {
            Add("app.title", "Open Universe — лаунчер", "Open Universe — launcher");
            Add("lang.label", "Язык:", "Language:");

            Add("tab.signin", "Вход", "Sign in");
            Add("tab.register", "Регистрация", "Register");
            Add("label.login", "Логин", "Login");
            Add("label.password", "Пароль", "Password");
            Add("label.callname", "Позывной (логин)", "Call sign (login)");
            Add("label.passwordMin", "Пароль (от 4 символов)", "Password (min 4 chars)");
            Add("label.repeat", "Повторите пароль", "Repeat password");
            Add("chk.remember", "Запомнить логин", "Remember login");
            Add("chk.rules", "Принимаю правила сообщества", "I accept the community rules");
            Add("btn.signin", "Войти", "Sign in");
            Add("btn.create", "Создать профиль", "Create profile");

            Add("signed.title", "Вы вошли в систему", "You are signed in");
            Add("signed.pilot", "Пилот: {0}", "Pilot: {0}");
            Add("signed.note", "Логин и пароль сохранены — вход выполняется автоматически.",
                "Login and password are saved — sign-in happens automatically.");

            Add("ver.installed", "Установлено:", "Installed:");
            Add("ver.latest", "Доступно:", "Available:");
            Add("ver.none", "не установлена", "not installed");
            Add("ver.checking", "проверяю версию…", "checking version…");

            Add("news.title", "Новости и обновления", "News and updates");
            Add("news.loading", "Загружаю новости…", "Loading news…");
            Add("news.noVersions", "Не удалось получить список версий.\r\nПроверьте интернет-соединение.",
                "Could not get the version list.\r\nCheck your internet connection.");
            Add("news.fail", "Не удалось загрузить новости.\r\nПроверьте интернет-соединение.",
                "Could not load the news.\r\nCheck your internet connection.");

            Add("btn.play", "▶ Играть", "▶ Play");
            Add("btn.playNotInstalled", "▶ Играть (не установлена)", "▶ Play (not installed)");
            Add("btn.playSignIn", "▶ Играть (войдите)", "▶ Play (sign in)");
            Add("btn.update", "⭳ Проверить обновления", "⭳ Check for updates");
            Add("btn.site", "Сайт", "Website");
            Add("btn.folder", "Папка игры", "Game folder");
            Add("btn.logout", "Выйти", "Sign out");

            Add("status.ready", "Готово к работе.", "Ready.");
            Add("status.checking", "Проверяю обновления…", "Checking for updates…");
            Add("status.upToDate", "Установлена актуальная версия {0} — обновление не требуется.",
                "You already have the latest version {0} — no update needed.");
            Add("status.updateAvail", "Доступно обновление: установлено {0}, новая версия {1}. Нажмите «Проверить обновления».",
                "Update available: installed {0}, new version {1}. Press “Check for updates”.");
            Add("status.notInstalled", "Игра не установлена. Доступна версия {0}.",
                "The game is not installed. Version {0} is available.");
            Add("status.noInstaller", "В релизах нет установщика игры.", "No game installer found in the releases.");
            Add("status.noVersions", "Не удалось получить список версий (нет сети?).",
                "Could not get the version list (no network?).");
            Add("status.downloading", "Скачиваю… {0:N0} / {1:N0} МБ", "Downloading… {0:N0} / {1:N0} MB");
            Add("status.downloading2", "Скачиваю… {0:N0} МБ", "Downloading… {0:N0} MB");
            Add("status.installing", "Устанавливаю игру в фоне…", "Installing the game in the background…");
            Add("status.installing2", "Идёт установка игры…", "Installing the game…");
            Add("status.installed", "Готово: игра установлена ({0}). Нажмите «Играть».",
                "Done: the game is installed ({0}). Press Play.");
            Add("status.installFail", "Не удалось установить игру: ", "Could not install the game: ");
            Add("status.launching", "Игра запускается…", "Launching the game…");
            Add("status.launchFail", "Не удалось запустить игру: ", "Could not launch the game: ");
            Add("status.updateError", "Ошибка проверки обновлений: ", "Update check error: ");
            Add("status.needSignIn", "Сначала войдите или зарегистрируйтесь.", "Please sign in or register first.");
            Add("status.noGameFile", "Игра не найдена. Нажмите «Проверить обновления».",
                "The game was not found. Press “Check for updates”.");

            Add("hint.profileFound", "Профиль найден: {0}. Введите пароль.", "Profile found: {0}. Enter your password.");
            Add("hint.signedIn", "Вы вошли как {0}.", "Signed in as {0}.");
            Add("hint.welcome", "Добро пожаловать, {0}!", "Welcome, {0}!");
            Add("hint.signedOut", "Вы вышли из профиля.", "You signed out.");
            Add("hint.created", "Профиль «{0}» создан. Можно играть.", "Profile “{0}” created. You can play.");

            Add("err.callnameShort", "Позывной — минимум 2 символа.", "Call sign must be at least 2 characters.");
            Add("err.callnameChars", "В позывном — только буквы, цифры, пробел, точка, дефис.",
                "The call sign may contain only letters, digits, spaces, dots and hyphens.");
            Add("err.passShort", "Пароль — минимум 4 символа.", "Password must be at least 4 characters.");
            Add("err.passMismatch", "Пароли не совпадают.", "Passwords do not match.");
            Add("err.needRules", "Нужно принять правила сообщества.", "You must accept the community rules.");
            Add("err.enterLogin", "Введите логин.", "Enter your login.");
            Add("err.enterPass", "Введите пароль.", "Enter your password.");
            Add("err.profileMissing", "Профиль не найден. Откройте вкладку «Регистрация».",
                "Profile not found. Open the “Register” tab.");
            Add("err.wrongPass", "Неверный пароль.", "Wrong password.");

            Add("prompt.title", "Обновление игры", "Game update");
            Add("prompt.download", "Игра не установлена.\r\n\r\nСкачать игру {0} ({1} МБ)?",
                "The game is not installed.\r\n\r\nDownload the game {0} ({1} MB)?");
            Add("prompt.update", "Доступно обновление игры {0} (установлена {1}).\r\n\r\nСкачать обновление?",
                "Game update {0} is available (installed {1}).\r\n\r\nDownload the update?");
        }

        private string T(string key)
        {
            string[] pair;
            if (S.TryGetValue(key, out pair)) return _lang == "en" ? pair[1] : pair[0];
            return key;
        }

        // ---------------- Поля интерфейса ----------------
        private Label _title, _langLabel, _verInstalled, _verLatest;
        private ComboBox _langCombo;

        private TabControl _authTabs;
        private TabPage _enterTab, _regTab;
        private Label _lblLogin, _lblPassword, _lblCallname, _lblPassMin, _lblRepeat;
        private TextBox _loginIn, _passIn;
        private TextBox _regLogin, _regPass, _regPass2;
        private CheckBox _regRules, _remember;
        private Button _enterBtn, _regBtn;
        private Label _authHint;

        private Panel _signedPanel;
        private Label _signedTitle, _signedPilot, _signedNote;

        private RichTextBox _news;
        private Label _newsTitle;
        private Button _play, _update, _site, _folder, _logout;
        private ProgressBar _progress;
        private Label _status;

        public LauncherForm()
        {
            _profilePath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "OpenUniverse", "launcher.ini");
            _settingsPath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "OpenUniverse", "settings.ini");

            // Язык: сохранённый выбор, иначе по языку системы.
            string saved = ReadSetting("lang");
            _lang = (saved == "ru" || saved == "en") ? saved : DetectDefaultLang();

            Text = T("app.title");
            ClientSize = new Size(820, 560);
            MinimumSize = new Size(720, 520);
            StartPosition = FormStartPosition.CenterScreen;
            BackColor = Color.FromArgb(10, 14, 30);
            ForeColor = Color.FromArgb(220, 228, 255);
            Font = new Font("Segoe UI", 9.5f);

            BuildUi();
            ApplyLanguage();
            LoadProfile();

            Shown += async (s, e) =>
            {
                await RefreshNewsAsync();
                await CheckForUpdateAsync(silent: true);
            };
        }

        private void BuildUi()
        {
            _title = new Label
            {
                Text = "OPEN UNIVERSE",
                Font = new Font("Segoe UI", 20f, FontStyle.Bold),
                ForeColor = Color.FromArgb(120, 160, 255),
                AutoSize = true,
                Location = new Point(24, 16)
            };
            Controls.Add(_title);

            // ---- Выбор языка (в правом верхнем углу) ----
            _langLabel = new Label
            {
                AutoSize = true,
                ForeColor = Color.FromArgb(160, 176, 210),
                Location = new Point(592, 22)
            };
            Controls.Add(_langLabel);

            _langCombo = new ComboBox
            {
                DropDownStyle = ComboBoxStyle.DropDownList,
                Location = new Point(658, 18),
                Size = new Size(138, 24),
                FlatStyle = FlatStyle.Flat,
                BackColor = Color.FromArgb(18, 24, 46),
                ForeColor = Color.FromArgb(220, 228, 255)
            };
            _langCombo.Items.Add("Русский");
            _langCombo.Items.Add("English");
            _langCombo.SelectedIndex = _lang == "en" ? 1 : 0;
            _langCombo.SelectedIndexChanged += (s, e) =>
            {
                _lang = _langCombo.SelectedIndex == 1 ? "en" : "ru";
                WriteSetting("lang", _lang);
                ApplyLanguage();
            };
            Controls.Add(_langCombo);

            // ---- Панель версий (установлено / доступно) ----
            var verPanel = new Panel
            {
                Location = new Point(560, 48),
                Size = new Size(236, 34),
                BackColor = Color.FromArgb(18, 24, 46)
            };
            Controls.Add(verPanel);

            _verInstalled = new Label
            {
                AutoSize = true,
                Location = new Point(8, 2),
                ForeColor = Color.FromArgb(210, 220, 245),
                Font = new Font("Segoe UI", 8.5f)
            };
            verPanel.Controls.Add(_verInstalled);

            _verLatest = new Label
            {
                AutoSize = true,
                Location = new Point(8, 18),
                ForeColor = Color.FromArgb(150, 168, 205),
                Font = new Font("Segoe UI", 8.5f)
            };
            verPanel.Controls.Add(_verLatest);

            // ---- Панель входа/регистрации (вкладки) ----
            _authTabs = new TabControl
            {
                Location = new Point(24, 86),
                Size = new Size(330, 300)
            };
            Controls.Add(_authTabs);

            // Вкладка «Вход».
            _enterTab = new TabPage("Вход") { BackColor = Color.FromArgb(18, 24, 46) };
            _authTabs.TabPages.Add(_enterTab);
            _lblLogin = MkLabel("Логин", 16, 20);
            _enterTab.Controls.Add(_lblLogin);
            _loginIn = new TextBox { Location = new Point(16, 42), Width = 280 };
            _enterTab.Controls.Add(_loginIn);
            _lblPassword = MkLabel("Пароль", 16, 80);
            _enterTab.Controls.Add(_lblPassword);
            _passIn = new TextBox { Location = new Point(16, 102), Width = 280, UseSystemPasswordChar = true };
            _enterTab.Controls.Add(_passIn);
            _remember = new CheckBox
            {
                Text = "Запомнить логин",
                Location = new Point(16, 136),
                AutoSize = true,
                ForeColor = Color.FromArgb(170, 185, 215)
            };
            _enterTab.Controls.Add(_remember);
            _enterBtn = MkButton("Войти", 16, 170, 280, Color.FromArgb(90, 130, 255), Color.White);
            _enterBtn.Click += OnSignIn;
            _enterTab.Controls.Add(_enterBtn);

            // Вкладка «Регистрация».
            _regTab = new TabPage("Регистрация") { BackColor = Color.FromArgb(18, 24, 46) };
            _authTabs.TabPages.Add(_regTab);
            _lblCallname = MkLabel("Позывной (логин)", 16, 14);
            _regTab.Controls.Add(_lblCallname);
            _regLogin = new TextBox { Location = new Point(16, 36), Width = 280 };
            _regTab.Controls.Add(_regLogin);
            _lblPassMin = MkLabel("Пароль (от 4 символов)", 16, 70);
            _regTab.Controls.Add(_lblPassMin);
            _regPass = new TextBox { Location = new Point(16, 92), Width = 280, UseSystemPasswordChar = true };
            _regTab.Controls.Add(_regPass);
            _lblRepeat = MkLabel("Повторите пароль", 16, 126);
            _regTab.Controls.Add(_lblRepeat);
            _regPass2 = new TextBox { Location = new Point(16, 148), Width = 280, UseSystemPasswordChar = true };
            _regTab.Controls.Add(_regPass2);
            _regRules = new CheckBox
            {
                Text = "Принимаю правила сообщества",
                Location = new Point(16, 180),
                AutoSize = true,
                ForeColor = Color.FromArgb(170, 185, 215)
            };
            _regTab.Controls.Add(_regRules);
            _regBtn = MkButton("Создать профиль", 16, 208, 280, Color.FromArgb(0, 200, 170), Color.FromArgb(4, 18, 26));
            _regBtn.Click += OnRegister;
            _regTab.Controls.Add(_regBtn);

            // ---- Панель «уже вошли в систему» (заменяет вкладки) ----
            _signedPanel = new Panel
            {
                Location = new Point(24, 86),
                Size = new Size(330, 300),
                BackColor = Color.FromArgb(18, 24, 46),
                Visible = false
            };
            Controls.Add(_signedPanel);
            _signedTitle = new Label
            {
                Text = "Вы вошли в систему",
                Font = new Font("Segoe UI", 12f, FontStyle.Bold),
                ForeColor = Color.FromArgb(0, 220, 180),
                AutoSize = true,
                Location = new Point(20, 24)
            };
            _signedPanel.Controls.Add(_signedTitle);
            _signedPilot = new Label
            {
                AutoSize = true,
                Location = new Point(20, 70),
                ForeColor = Color.FromArgb(220, 228, 255),
                Font = new Font("Segoe UI", 11f, FontStyle.Bold)
            };
            _signedPanel.Controls.Add(_signedPilot);
            _signedNote = new Label
            {
                Size = new Size(290, 80),
                Location = new Point(20, 100),
                ForeColor = Color.FromArgb(150, 168, 205)
            };
            _signedPanel.Controls.Add(_signedNote);

            _authHint = new Label
            {
                Location = new Point(24, 390),
                Size = new Size(330, 40),
                ForeColor = Color.FromArgb(150, 168, 205)
            };
            Controls.Add(_authHint);

            // ---- Новости ----
            var newsPanel = new Panel
            {
                Location = new Point(372, 86),
                Size = new Size(424, 300),
                BackColor = Color.FromArgb(18, 24, 46)
            };
            Controls.Add(newsPanel);

            _newsTitle = new Label
            {
                Text = "Новости и обновления",
                Font = new Font("Segoe UI", 12f, FontStyle.Bold),
                ForeColor = Color.FromArgb(200, 214, 255),
                AutoSize = true,
                Location = new Point(14, 10)
            };
            newsPanel.Controls.Add(_newsTitle);

            _news = new RichTextBox
            {
                Location = new Point(10, 40),
                Size = new Size(404, 250),
                ReadOnly = true,
                BorderStyle = BorderStyle.None,
                BackColor = Color.FromArgb(18, 24, 46),
                ForeColor = Color.FromArgb(205, 216, 240),
                ScrollBars = RichTextBoxScrollBars.Vertical
            };
            newsPanel.Controls.Add(_news);

            // ---- Кнопки действий ----
            _play = MkButton("▶ Играть", 24, 442, 170, Color.FromArgb(0, 200, 170), Color.FromArgb(4, 18, 26));
            _play.Enabled = false;
            _play.Click += OnPlay;
            Controls.Add(_play);

            _update = MkButton("⭳ Проверить обновления", 204, 442, 210, Color.FromArgb(30, 40, 80), Color.FromArgb(200, 214, 255));
            _update.Click += async (s, e) => await CheckForUpdateAsync(silent: false);
            Controls.Add(_update);

            _site = MkButton("Сайт", 424, 442, 90, Color.FromArgb(30, 40, 80), Color.FromArgb(200, 214, 255));
            _site.Click += (s, e) => Open(SiteUrl);
            Controls.Add(_site);

            _folder = MkButton("Папка игры", 524, 442, 120, Color.FromArgb(30, 40, 80), Color.FromArgb(200, 214, 255));
            _folder.Click += (s, e) =>
            {
                var dir = GameDir();
                Directory.CreateDirectory(dir);
                Open("explorer.exe", "\"" + dir + "\"");
            };
            Controls.Add(_folder);

            _logout = MkButton("Выйти", 654, 442, 142, Color.FromArgb(30, 40, 80), Color.FromArgb(200, 214, 255));
            _logout.Click += OnSignOut;
            Controls.Add(_logout);

            _progress = new ProgressBar
            {
                Location = new Point(24, 498),
                Size = new Size(772, 14),
                Style = ProgressBarStyle.Continuous,
                Visible = false
            };
            Controls.Add(_progress);

            _status = new Label
            {
                Text = "Готово к работе.",
                ForeColor = Color.FromArgb(150, 168, 205),
                AutoSize = false,
                Size = new Size(772, 34),
                Location = new Point(24, 516)
            };
            Controls.Add(_status);
        }

        private static Label MkLabel(string text, int x, int y)
        {
            return new Label
            {
                Text = text,
                Location = new Point(x, y),
                AutoSize = true,
                ForeColor = Color.FromArgb(160, 176, 210)
            };
        }

        private static Button MkButton(string text, int x, int y, int w, Color back, Color fore)
        {
            var b = new Button
            {
                Text = text,
                Location = new Point(x, y),
                Size = new Size(w, 40),
                FlatStyle = FlatStyle.Flat,
                BackColor = back,
                ForeColor = fore,
                Font = new Font("Segoe UI", 10f, FontStyle.Bold)
            };
            b.FlatAppearance.BorderSize = 0;
            return b;
        }

        // ---------------- Язык ----------------

        private static string DetectDefaultLang()
        {
            try
            {
                string two = System.Globalization.CultureInfo.CurrentUICulture.TwoLetterISOLanguageName;
                if (two == "ru") return "ru";
            }
            catch { }
            return "en";
        }

        private void ApplyLanguage()
        {
            Text = T("app.title");
            _langLabel.Text = T("lang.label");
            _enterTab.Text = T("tab.signin");
            _regTab.Text = T("tab.register");
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
            _newsTitle.Text = T("news.title");
            _update.Text = T("btn.update");
            _site.Text = T("btn.site");
            _folder.Text = T("btn.folder");
            _logout.Text = T("btn.logout");

            // Статус-строку и подсказку тоже переводим, если они в состоянии покоя.
            if (string.IsNullOrEmpty(_status.Text) || _status.Text == "Готово к работе." || _status.Text == "Ready.")
                _status.Text = T("status.ready");

            UpdateVersionLabels();
            UpdatePlayButton();
            if (_signedPanel.Visible && _profile != null)
                _signedPilot.Text = string.Format(T("signed.pilot"), _profile.Login);
        }

        private void ShowSignedIn()
        {
            _authTabs.Visible = false;
            _signedPanel.Visible = true;
            _signedPilot.Text = string.Format(T("signed.pilot"), _profile.Login);
        }

        private void ShowSignedOut()
        {
            _signedPanel.Visible = false;
            _authTabs.Visible = true;
        }

        // ---------------- Профиль ----------------

        private void LoadProfile()
        {
            _profile = ReadProfile();
            if (_profile != null && !string.IsNullOrEmpty(_profile.PasswordHash))
            {
                // Логин и пароль уже вводили ранее — входим автоматически.
                _loginIn.Text = _profile.Login;
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
                {
                    foreach (var line in File.ReadAllLines(_settingsPath))
                    {
                        int i = line.IndexOf('=');
                        if (i > 0) dict[line.Substring(0, i)] = line.Substring(i + 1);
                    }
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
            string login = (_regLogin.Text ?? "").Trim();
            string p1 = _regPass.Text ?? "";
            string p2 = _regPass2.Text ?? "";
            if (login.Length < 2) { SetHint(T("err.callnameShort"), true); return; }
            if (!System.Text.RegularExpressions.Regex.IsMatch(login, "^[A-Za-z0-9_.\\-А-Яа-яЁё ]+$"))
            { SetHint(T("err.callnameChars"), true); return; }
            if (p1.Length < 4) { SetHint(T("err.passShort"), true); return; }
            if (p1 != p2) { SetHint(T("err.passMismatch"), true); return; }
            if (!_regRules.Checked) { SetHint(T("err.needRules"), true); return; }

            _profile = new PilotProfile { Login = login, PasswordHash = Hash(p1), Remember = true };
            SaveProfile(_profile);
            SetHint(string.Format(T("hint.created"), login), false);
            _loginIn.Text = login;
            ShowSignedIn();
            UpdatePlayButton();
        }

        private void OnSignIn(object sender, EventArgs e)
        {
            string login = (_loginIn.Text ?? "").Trim();
            string pass = _passIn.Text ?? "";
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
            _passIn.Text = "";
            ShowSignedOut();
            SetHint(T("hint.signedOut"), false);
            UpdatePlayButton();
        }

        private void SetHint(string text, bool error)
        {
            _authHint.Text = text;
            _authHint.ForeColor = error ? Color.FromArgb(255, 130, 130) : Color.FromArgb(120, 220, 180);
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
                    UpdateVersionLabels();
                    if (!silent) SetStatus(T("status.noVersions"), true);
                    return;
                }

                var latest = _releases[0];

                // Установщик ищем в ближайшем релизе, где он есть:
                // последние облегчённые релизы могут содержать только лаунчер.
                var installable = FindInstallable(_releases);

                // Версия игры — это версия установщика (может отличаться от тега
                // последнего релиза, если свежие релизы содержат только лаунчер).
                string gameTag = installable == null
                    ? latest.Tag
                    : (string.IsNullOrEmpty(installable.GameSetupTag) ? installable.Tag : installable.GameSetupTag);

                _installedDisplay = InstalledVersion();
                _latestGameDisplay = gameTag;
                UpdateVersionLabels();

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
                    // Версии совпадают — ничего не скачиваем.
                    if (!silent) SetStatus(string.Format(T("status.upToDate"), Pretty(gameTag)), false);
                    UpdatePlayButton();
                    return;
                }

                // Автопроверка при запуске только показывает статус, без вопроса
                // и без скачивания — установку запускает кнопка.
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
                {
                    what = string.Format(T("prompt.download"), Pretty(gameTag),
                        installable.GameSetupSizeMb.ToString("0"));
                }
                else
                {
                    what = string.Format(T("prompt.update"), Pretty(gameTag), Pretty(_installedDisplay));
                }

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
                        {
                            SetStatus(string.Format(T("status.downloading"),
                                e.BytesReceived / 1048576.0, e.TotalBytesToReceive / 1048576.0), false);
                        }
                        else
                        {
                            SetStatus(string.Format(T("status.downloading2"), e.BytesReceived / 1048576.0), false);
                        }
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
                // Тихая установка: без окон и вопросов. Установщик сам НЕ запускает
                // лаунчер (флаг skipifsilent), поэтому второго окна не будет.
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
                    // Ждём завершения установки, не блокируя интерфейс.
                    SetStatus(T("status.installing2"), false);
                    await Task.Run(() => inst.WaitForExit());
                }

                _installedDisplay = InstalledVersion();
                UpdateVersionLabels();
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

        /// <summary>Нормализованная версия: без ведущей «v» и пробелов.</summary>
        private static string Norm(string tag)
        {
            if (string.IsNullOrEmpty(tag)) return "";
            return tag.Trim().TrimStart('v', 'V');
        }

        /// <summary>Версия для показа: с ведущей «v».</summary>
        private static string Pretty(string tag)
        {
            if (string.IsNullOrEmpty(tag)) return "—";
            if (tag[0] == 'v' || tag[0] == 'V') return tag;
            return "v" + tag;
        }

        private void UpdateVersionLabels()
        {
            string inst = string.IsNullOrEmpty(_installedDisplay)
                ? T("ver.none") : Pretty(_installedDisplay);
            string latest = string.IsNullOrEmpty(_latestGameDisplay)
                ? "—" : Pretty(_latestGameDisplay);

            if (_verInstalled == null) return;
            _verInstalled.Text = T("ver.installed") + " " + inst;
            _verLatest.Text = T("ver.latest") + " " + latest;

            bool upToDate = !string.IsNullOrEmpty(_installedDisplay)
                && !string.IsNullOrEmpty(_latestGameDisplay)
                && Norm(_installedDisplay) == Norm(_latestGameDisplay);

            if (string.IsNullOrEmpty(_latestGameDisplay))
                _verLatest.ForeColor = Color.FromArgb(150, 168, 205);
            else if (upToDate)
                _verLatest.ForeColor = Color.FromArgb(90, 220, 160);
            else
                _verLatest.ForeColor = Color.FromArgb(255, 200, 90);
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
                Process.Start(new ProcessStartInfo { FileName = exe, WorkingDirectory = Path.GetDirectoryName(exe) });
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
            _status.ForeColor = error ? Color.FromArgb(255, 130, 130) : Color.FromArgb(150, 168, 205);
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

        /// <summary>Загрузить список релизов: сначала с сайта, потом GitHub API.</summary>
        private async Task<List<ReleaseInfo>> LoadReleasesAsync()
        {
            var list = new List<ReleaseInfo>();
            // 1) Статический файл сайта — без лимитов и работает всегда.
            try
            {
                string json = await HttpGetAsync(RepoJson);
                list = ParseSiteJson(json);
                if (list.Count > 0) return list;
            }
            catch { /* перейдём к API */ }

            // 2) GitHub API — резерв.
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
                client.DefaultRequestHeaders.UserAgent.ParseAdd("OpenUniverseLauncher/1.3");
                return await client.GetStringAsync(url);
            }
        }

        /// <summary>Разбор releases.json сайта (свой простой формат).</summary>
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

        /// <summary>Разбор ответа GitHub API (вложенный JSON).</summary>
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

        /// <summary>Ближайший релиз, в котором есть установщик игры.</summary>
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
