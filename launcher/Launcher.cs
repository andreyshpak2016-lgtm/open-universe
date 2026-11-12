// Open Universe — лаунчер.
//
// Возможности:
//   * регистрация и вход по логину и паролю (профиль хранится локально);
//   * новости об обновлениях (тела релизов GitHub прямо в окне);
//   * автообновление: сравнение версии и скачивание установщика игры;
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
        private PilotProfile _profile;
        private readonly List<ReleaseInfo> _releases = new List<ReleaseInfo>();

        // Вход / регистрация.
        private TabControl _authTabs;
        private TextBox _loginIn, _passIn;
        private TextBox _regLogin, _regPass, _regPass2;
        private CheckBox _regRules, _remember;
        private Label _authHint;

        // Новости и действия.
        private RichTextBox _news;
        private Button _play, _update;
        private ProgressBar _progress;
        private Label _status, _version;

        public LauncherForm()
        {
            _profilePath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "OpenUniverse", "launcher.ini");

            Text = "Open Universe — лаунчер";
            ClientSize = new Size(820, 560);
            MinimumSize = new Size(720, 520);
            StartPosition = FormStartPosition.CenterScreen;
            BackColor = Color.FromArgb(10, 14, 30);
            ForeColor = Color.FromArgb(220, 228, 255);
            Font = new Font("Segoe UI", 9.5f);

            BuildUi();
            LoadProfile();

            Shown += async (s, e) =>
            {
                await RefreshNewsAsync();
                await CheckForUpdateAsync(silent: true);
            };
        }

        private void BuildUi()
        {
            var title = new Label
            {
                Text = "OPEN UNIVERSE",
                Font = new Font("Segoe UI", 20f, FontStyle.Bold),
                ForeColor = Color.FromArgb(120, 160, 255),
                AutoSize = true,
                Location = new Point(24, 16)
            };
            Controls.Add(title);

            _version = new Label
            {
                Text = "проверяю версию…",
                ForeColor = Color.FromArgb(130, 145, 180),
                AutoSize = true,
                Location = new Point(28, 54)
            };
            Controls.Add(_version);

            // ---- Панель входа/регистрации (вкладки) ----
            _authTabs = new TabControl
            {
                Location = new Point(24, 86),
                Size = new Size(330, 300)
            };
            Controls.Add(_authTabs);

            // Вкладка «Вход».
            var enterTab = new TabPage("Вход") { BackColor = Color.FromArgb(18, 24, 46) };
            _authTabs.TabPages.Add(enterTab);
            enterTab.Controls.Add(MkLabel("Логин", 16, 20));
            _loginIn = new TextBox { Location = new Point(16, 42), Width = 280 };
            enterTab.Controls.Add(_loginIn);
            enterTab.Controls.Add(MkLabel("Пароль", 16, 80));
            _passIn = new TextBox { Location = new Point(16, 102), Width = 280, UseSystemPasswordChar = true };
            enterTab.Controls.Add(_passIn);
            _remember = new CheckBox
            {
                Text = "Запомнить логин",
                Location = new Point(16, 136),
                AutoSize = true,
                ForeColor = Color.FromArgb(170, 185, 215)
            };
            enterTab.Controls.Add(_remember);
            var enterBtn = MkButton("Войти", 16, 170, 280, Color.FromArgb(90, 130, 255), Color.White);
            enterBtn.Click += OnSignIn;
            enterTab.Controls.Add(enterBtn);

            // Вкладка «Регистрация».
            var regTab = new TabPage("Регистрация") { BackColor = Color.FromArgb(18, 24, 46) };
            _authTabs.TabPages.Add(regTab);
            regTab.Controls.Add(MkLabel("Позывной (логин)", 16, 14));
            _regLogin = new TextBox { Location = new Point(16, 36), Width = 280 };
            regTab.Controls.Add(_regLogin);
            regTab.Controls.Add(MkLabel("Пароль (от 4 символов)", 16, 70));
            _regPass = new TextBox { Location = new Point(16, 92), Width = 280, UseSystemPasswordChar = true };
            regTab.Controls.Add(_regPass);
            regTab.Controls.Add(MkLabel("Повторите пароль", 16, 126));
            _regPass2 = new TextBox { Location = new Point(16, 148), Width = 280, UseSystemPasswordChar = true };
            regTab.Controls.Add(_regPass2);
            _regRules = new CheckBox
            {
                Text = "Принимаю правила сообщества",
                Location = new Point(16, 180),
                AutoSize = true,
                ForeColor = Color.FromArgb(170, 185, 215)
            };
            regTab.Controls.Add(_regRules);
            var regBtn = MkButton("Создать профиль", 16, 208, 280, Color.FromArgb(0, 200, 170), Color.FromArgb(4, 18, 26));
            regBtn.Click += OnRegister;
            regTab.Controls.Add(regBtn);

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

            var newsTitle = new Label
            {
                Text = "Новости и обновления",
                Font = new Font("Segoe UI", 12f, FontStyle.Bold),
                ForeColor = Color.FromArgb(200, 214, 255),
                AutoSize = true,
                Location = new Point(14, 10)
            };
            newsPanel.Controls.Add(newsTitle);

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

            var site = MkButton("Сайт", 424, 442, 90, Color.FromArgb(30, 40, 80), Color.FromArgb(200, 214, 255));
            site.Click += (s, e) => Open(SiteUrl);
            Controls.Add(site);

            var folder = MkButton("Папка игры", 524, 442, 120, Color.FromArgb(30, 40, 80), Color.FromArgb(200, 214, 255));
            folder.Click += (s, e) =>
            {
                var dir = GameDir();
                Directory.CreateDirectory(dir);
                Open("explorer.exe", "\"" + dir + "\"");
            };
            Controls.Add(folder);

            var logout = MkButton("Выйти", 654, 442, 142, Color.FromArgb(30, 40, 80), Color.FromArgb(200, 214, 255));
            logout.Click += (s, e) =>
            {
                _profile = null;
                try { if (File.Exists(_profilePath)) File.Delete(_profilePath); } catch { }
                _authHint.Text = "Вы вышли из профиля.";
                UpdatePlayButton();
            };
            Controls.Add(logout);

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

        // ---------------- Профиль ----------------

        private void LoadProfile()
        {
            _profile = ReadProfile();
            if (_profile != null)
            {
                _loginIn.Text = _profile.Login;
                _remember.Checked = _profile.Remember;
                _authHint.Text = "Профиль найден: " + _profile.Login + ". Введите пароль.";
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
            if (login.Length < 2) { SetHint("Позывной — минимум 2 символа.", true); return; }
            if (!System.Text.RegularExpressions.Regex.IsMatch(login, "^[A-Za-z0-9_.\\-А-Яа-яЁё ]+$"))
            { SetHint("В позывном — только буквы, цифры, пробел, точка, дефис.", true); return; }
            if (p1.Length < 4) { SetHint("Пароль — минимум 4 символа.", true); return; }
            if (p1 != p2) { SetHint("Пароли не совпадают.", true); return; }
            if (!_regRules.Checked) { SetHint("Нужно принять правила сообщества.", true); return; }

            _profile = new PilotProfile { Login = login, PasswordHash = Hash(p1), Remember = true };
            SaveProfile(_profile);
            SetHint("Профиль «" + login + "» создан. Можно играть.", false);
            _authTabs.SelectedIndex = 0;
            _loginIn.Text = login;
            UpdatePlayButton();
        }

        private void OnSignIn(object sender, EventArgs e)
        {
            string login = (_loginIn.Text ?? "").Trim();
            string pass = _passIn.Text ?? "";
            if (login.Length < 2) { SetHint("Введите логин.", true); return; }
            if (pass.Length < 1) { SetHint("Введите пароль.", true); return; }

            var stored = ReadProfile();
            if (stored == null || !string.Equals(stored.Login, login, StringComparison.OrdinalIgnoreCase))
            {
                SetHint("Профиль не найден. Откройте вкладку «Регистрация».", true);
                return;
            }
            if (!string.Equals(stored.PasswordHash, Hash(pass), StringComparison.Ordinal))
            {
                SetHint("Неверный пароль.", true);
                return;
            }

            stored.Remember = _remember.Checked;
            _profile = stored;
            SaveProfile(stored);
            SetHint("Добро пожаловать, " + stored.Login + "!", false);
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
            _news.Text = "Загружаю новости…";
            try
            {
                _releases.Clear();
                _releases.AddRange(await LoadReleasesAsync());

                if (_releases.Count == 0)
                {
                    _news.Text = "Не удалось получить список версий.\r\nПроверьте интернет-соединение.";
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
                _version.Text = "последняя версия: " + _releases[0].Tag;
            }
            catch (Exception ex)
            {
                _news.Text = "Не удалось загрузить новости.\r\nПроверьте интернет-соединение.\r\n\r\n(" + ex.Message + ")";
            }
        }

        // ---------------- Обновления ----------------

        private async Task CheckForUpdateAsync(bool silent)
        {
            if (!silent) SetStatus("Проверяю обновления…", false);
            try
            {
                if (_releases.Count == 0)
                    _releases.AddRange(await LoadReleasesAsync());
                if (_releases.Count == 0)
                {
                    if (!silent) SetStatus("Не удалось получить список версий (нет сети?).", true);
                    return;
                }

                var latest = _releases[0];
                _version.Text = "последняя версия: " + latest.Tag;

                // Установщик ищем в ближайшем релизе, где он есть:
                // последние облегчённые релизы могут содержать только лаунчер.
                var installable = FindInstallable(_releases);
                if (installable == null)
                {
                    if (!silent) SetStatus("В релизах нет установщика игры.", true);
                    UpdatePlayButton();
                    return;
                }

                // Версия игры — это версия установщика (может быть старше последнего релиза,
                // если свежие релизы содержат только лаунчер).
                string gameTag = string.IsNullOrEmpty(installable.GameSetupTag)
                    ? installable.Tag : installable.GameSetupTag;
                string installed = InstalledVersion();
                if (!string.IsNullOrEmpty(installed) && installed == gameTag)
                {
                    if (!silent) SetStatus("Установлена актуальная игра (" + installed + ").", false);
                    UpdatePlayButton();
                    return;
                }

                string what;
                if (string.IsNullOrEmpty(installed))
                {
                    what = "Игра не установлена.\r\n\r\nСкачать игру " + gameTag +
                           " (" + installable.GameSetupSizeMb.ToString("0") + " МБ)?";
                }
                else
                {
                    what = "Доступно обновление игры " + gameTag +
                           " (установлена " + installed + ").\r\n\r\nСкачать обновление?";
                }

                if (MessageBox.Show(this, what, "Обновление игры",
                        MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes)
                {
                    UpdatePlayButton();
                    return;
                }

                await DownloadAndRunInstallerAsync(installable);
            }
            catch (Exception ex)
            {
                if (!silent) SetStatus("Ошибка проверки обновлений: " + ex.Message, true);
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
                            SetStatus(string.Format("Скачиваю… {0:N0} / {1:N0} МБ",
                                e.BytesReceived / 1048576.0, e.TotalBytesToReceive / 1048576.0), false);
                        }
                        else
                        {
                            SetStatus(string.Format("Скачиваю… {0:N0} МБ", e.BytesReceived / 1048576.0), false);
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

                SetStatus("Устанавливаю игру в фоне…", false);
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
                    SetStatus("Идёт установка игры…", false);
                    await Task.Run(() => inst.WaitForExit());
                }

                SetStatus("Готово: игра установлена (" + rel.GameSetupTag + "). Нажмите «Играть».", false);
                UpdatePlayButton();
            }
            catch (Exception ex)
            {
                SetStatus("Не удалось установить игру: " + ex.Message, true);
            }
            finally
            {
                _progress.Visible = false;
            }
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
            if (!hasGame) _play.Text = "▶ Играть (не установлена)";
            else if (!signed) _play.Text = "▶ Играть (войдите)";
            else _play.Text = "▶ Играть";
        }

        private void OnPlay(object sender, EventArgs e)
        {
            if (_profile == null) { SetStatus("Сначала войдите или зарегистрируйтесь.", true); return; }
            string exe = GameExe();
            if (exe == null) { SetStatus("Игра не найдена. Нажмите «Проверить обновления».", true); return; }
            try
            {
                Process.Start(new ProcessStartInfo { FileName = exe, WorkingDirectory = Path.GetDirectoryName(exe) });
                SetStatus("Игра запускается…", false);
            }
            catch (Exception ex)
            {
                SetStatus("Не удалось запустить игру: " + ex.Message, true);
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
                client.DefaultRequestHeaders.UserAgent.ParseAdd("OpenUniverseLauncher/1.2");
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
                    Notes = Str(d, "name"),
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
    }
}
