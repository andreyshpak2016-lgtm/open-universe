// Open Universe — лаунчер.
//
// Что делает:
//   * вход по логину и паролю (профиль создаётся локально, пароль хранится хешем);
//   * показывает новости обновлений (CHANGELOG.md из релиза);
//   * проверяет последнюю версию на GitHub и, если вышла новая, скачивает и
//     устанавливает установщик игры (автообновление);
//   * запускает установленную игру OpenUniverse.exe.
//
// Сборка (Windows, .NET Framework 4.x, без внешних зависимостей):
//   csc /target:winexe /out:OpenUniverseLauncher.exe /r:System.Windows.Forms.dll
//       /r:System.Drawing.dll /r:System.Net.Http.dll launcher/Launcher.cs

using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.IO.Compression;
using System.Net;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace OpenUniverseLauncher
{
    internal static class Program
    {
        [STAThread]
        private static void Main()
        {
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Application.Run(new LauncherForm());
        }
    }

    /// <summary>Профиль игрока: хранится локально, пароль — только хеш.</summary>
    internal sealed class PilotProfile
    {
        public string Login { get; set; }
        public string PasswordHash { get; set; }
        public bool Remember { get; set; }
    }

    internal sealed class LauncherForm : Form
    {
        // Где лежат сборки и описания релизов.
        private const string RepoApi = "https://api.github.com/repos/andreyshpak2016-lgtm/open-universe/releases?per_page=10";
        private const string SiteUrl = "https://andreyshpak2016-lgtm.github.io/open-universe/";

        private readonly string _profilePath;
        private PilotProfile _profile;

        private TextBox _login;
        private TextBox _password;
        private CheckBox _remember;
        private Button _signIn;
        private Label _status;
        private RichTextBox _news;
        private Button _play;
        private Button _update;
        private ProgressBar _progress;
        private Label _version;

        public LauncherForm()
        {
            _profilePath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "OpenUniverse", "launcher.json");

            Text = "Open Universe — лаунчер";
            ClientSize = new Size(760, 520);
            MinimumSize = new Size(640, 460);
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
            // Заголовок.
            var title = new Label
            {
                Text = "OPEN UNIVERSE",
                Font = new Font("Segoe UI", 20f, FontStyle.Bold),
                ForeColor = Color.FromArgb(120, 160, 255),
                AutoSize = true,
                Location = new Point(24, 18)
            };
            Controls.Add(title);

            _version = new Label
            {
                Text = "проверяю версию…",
                ForeColor = Color.FromArgb(130, 145, 180),
                AutoSize = true,
                Location = new Point(28, 58)
            };
            Controls.Add(_version);

            // --- Панель входа ---
            var signPanel = new Panel
            {
                Location = new Point(24, 92),
                Size = new Size(300, 250),
                BackColor = Color.FromArgb(18, 24, 46)
            };
            Controls.Add(signPanel);

            var signTitle = new Label
            {
                Text = "Вход пилота",
                Font = new Font("Segoe UI", 13f, FontStyle.Bold),
                ForeColor = Color.FromArgb(200, 214, 255),
                AutoSize = true,
                Location = new Point(16, 14)
            };
            signPanel.Controls.Add(signTitle);

            signPanel.Controls.Add(MkLabel("Логин", 16, 52));
            _login = new TextBox { Location = new Point(16, 72), Width = 268 };
            signPanel.Controls.Add(_login);

            signPanel.Controls.Add(MkLabel("Пароль", 16, 108));
            _password = new TextBox { Location = new Point(16, 128), Width = 268, UseSystemPasswordChar = true };
            signPanel.Controls.Add(_password);

            _remember = new CheckBox
            {
                Text = "Запомнить меня",
                Location = new Point(16, 158),
                AutoSize = true,
                ForeColor = Color.FromArgb(170, 185, 215)
            };
            signPanel.Controls.Add(_remember);

            _signIn = new Button
            {
                Text = "Войти",
                Location = new Point(16, 188),
                Size = new Size(268, 38),
                FlatStyle = FlatStyle.Flat,
                BackColor = Color.FromArgb(90, 130, 255),
                ForeColor = Color.White
            };
            _signIn.FlatAppearance.BorderSize = 0;
            _signIn.Click += OnSignIn;
            signPanel.Controls.Add(_signIn);

            // --- Новости ---
            var newsPanel = new Panel
            {
                Location = new Point(340, 92),
                Size = new Size(396, 250),
                BackColor = Color.FromArgb(18, 24, 46)
            };
            Controls.Add(newsPanel);

            var newsTitle = new Label
            {
                Text = "Новости и обновления",
                Font = new Font("Segoe UI", 12f, FontStyle.Bold),
                ForeColor = Color.FromArgb(200, 214, 255),
                AutoSize = true,
                Location = new Point(16, 12)
            };
            newsPanel.Controls.Add(newsTitle);

            _news = new RichTextBox
            {
                Location = new Point(12, 44),
                Size = new Size(372, 196),
                ReadOnly = true,
                BorderStyle = BorderStyle.None,
                BackColor = Color.FromArgb(18, 24, 46),
                ForeColor = Color.FromArgb(205, 216, 240),
                ScrollBars = RichTextBoxScrollBars.Vertical
            };
            newsPanel.Controls.Add(_news);

            // --- Кнопки действий ---
            _play = new Button
            {
                Text = "▶ Играть",
                Location = new Point(24, 366),
                Size = new Size(170, 46),
                Enabled = false,
                FlatStyle = FlatStyle.Flat,
                BackColor = Color.FromArgb(0, 200, 170),
                ForeColor = Color.FromArgb(4, 18, 26),
                Font = new Font("Segoe UI", 11f, FontStyle.Bold)
            };
            _play.FlatAppearance.BorderSize = 0;
            _play.Click += OnPlay;
            Controls.Add(_play);

            _update = new Button
            {
                Text = "⭳ Проверить обновления",
                Location = new Point(204, 366),
                Size = new Size(210, 46),
                FlatStyle = FlatStyle.Flat,
                BackColor = Color.FromArgb(30, 40, 80),
                ForeColor = Color.FromArgb(200, 214, 255)
            };
            _update.FlatAppearance.BorderSize = 0;
            _update.Click += async (s, e) => await CheckForUpdateAsync(silent: false);
            Controls.Add(_update);

            var site = new Button
            {
                Text = "Сайт",
                Location = new Point(424, 366),
                Size = new Size(90, 46),
                FlatStyle = FlatStyle.Flat,
                BackColor = Color.FromArgb(30, 40, 80),
                ForeColor = Color.FromArgb(200, 214, 255)
            };
            site.FlatAppearance.BorderSize = 0;
            site.Click += (s, e) => Process.Start(SiteUrl);
            Controls.Add(site);

            var reg = new Button
            {
                Text = "Регистрация",
                Location = new Point(524, 366),
                Size = new Size(120, 46),
                FlatStyle = FlatStyle.Flat,
                BackColor = Color.FromArgb(30, 40, 80),
                ForeColor = Color.FromArgb(200, 214, 255)
            };
            reg.FlatAppearance.BorderSize = 0;
            reg.Click += (s, e) => Process.Start(SiteUrl + "#register");
            Controls.Add(reg);

            var folder = new Button
            {
                Text = "Папка игры",
                Location = new Point(654, 366),
                Size = new Size(82, 46),
                FlatStyle = FlatStyle.Flat,
                BackColor = Color.FromArgb(30, 40, 80),
                ForeColor = Color.FromArgb(200, 214, 255)
            };
            folder.FlatAppearance.BorderSize = 0;
            folder.Click += (s, e) =>
            {
                string dir = GameDir();
                Directory.CreateDirectory(dir);
                Process.Start("explorer.exe", "\"" + dir + "\"");
            };
            Controls.Add(folder);

            // --- Прогресс и статус ---
            _progress = new ProgressBar
            {
                Location = new Point(24, 424),
                Size = new Size(712, 16),
                Style = ProgressBarStyle.Continuous,
                Visible = false
            };
            Controls.Add(_progress);

            _status = new Label
            {
                Text = "Готово к работе.",
                ForeColor = Color.FromArgb(150, 168, 205),
                AutoSize = false,
                Size = new Size(712, 40),
                Location = new Point(24, 444)
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

        // ---------------- Профиль ----------------

        private void LoadProfile()
        {
            _profile = ReadProfile(_profilePath);
            if (_profile != null && _profile.Remember)
            {
                _login.Text = _profile.Login;
                _remember.Checked = true;
                _status.Text = "Профиль найден. Введите пароль и нажмите «Войти».";
            }
        }

        private static PilotProfile ReadProfile(string path)
        {
            try
            {
                if (!File.Exists(path)) return null;
                var lines = File.ReadAllLines(path);
                var p = new PilotProfile();
                foreach (var l in lines)
                {
                    int i = l.IndexOf('=');
                    if (i <= 0) continue;
                    string k = l.Substring(0, i), v = l.Substring(i + 1);
                    if (k == "login") p.Login = v;
                    else if (k == "hash") p.PasswordHash = v;
                    else if (k == "remember") p.Remember = v == "1";
                }
                return p;
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
            catch { /* профиль — не критично */ }
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

        private void OnSignIn(object sender, EventArgs e)
        {
            string login = (_login.Text ?? "").Trim();
            string pass = _password.Text ?? "";
            if (login.Length < 2) { SetStatus("Логин — минимум 2 символа.", true); return; }
            if (pass.Length < 4) { SetStatus("Пароль — минимум 4 символа.", true); return; }

            string hash = Hash(pass);
            var existing = ReadProfile(_profilePath);
            if (existing != null && string.Equals(existing.Login, login, StringComparison.OrdinalIgnoreCase))
            {
                // Профиль уже есть — проверяем пароль.
                if (!string.Equals(existing.PasswordHash, hash, StringComparison.Ordinal))
                {
                    SetStatus("Неверный пароль для этого логина.", true);
                    return;
                }
                _profile = existing;
            }
            else
            {
                _profile = new PilotProfile { Login = login, PasswordHash = hash };
            }
            _profile.Remember = _remember.Checked;
            SaveProfile(_profile);

            SetStatus("Добро пожаловать, " + _profile.Login + "!", false);
            UpdatePlayButton();
        }

        // ---------------- Новости и обновления ----------------

        private async Task RefreshNewsAsync()
        {
            _news.Text = "Загружаю новости…";
            try
            {
                string json = await HttpGetAsync(RepoApi);
                var releases = ParseReleases(json);
                if (releases.Count == 0) { _news.Text = "Новостей пока нет."; return; }

                var sb = new StringBuilder();
                int shown = 0;
                foreach (var r in releases)
                {
                    sb.AppendLine("● " + r.Tag + "   " + r.Date.ToString("dd.MM.yyyy"));
                    if (!string.IsNullOrWhiteSpace(r.Notes))
                    {
                        string notes = r.Notes.Trim();
                        if (notes.Length > 320) notes = notes.Substring(0, 320) + "…";
                        sb.AppendLine(notes);
                    }
                    sb.AppendLine();
                    if (++shown >= 4) break;
                }
                _news.Text = sb.ToString();
                _version.Text = "последняя версия: " + releases[0].Tag;
            }
            catch (Exception ex)
            {
                _news.Text = "Не удалось загрузить новости (нет сети?).\r\n" + ex.Message;
            }
        }

        private async Task CheckForUpdateAsync(bool silent)
        {
            if (!silent) SetStatus("Проверяю обновления…", false);
            try
            {
                string json = await HttpGetAsync(RepoApi);
                var releases = ParseReleases(json);
                if (releases.Count == 0) { if (!silent) SetStatus("Релизы не найдены.", true); return; }

                Release latest = releases[0];
                _version.Text = "последняя версия: " + latest.Tag;

                string installed = InstalledVersion();
                if (!string.IsNullOrEmpty(installed) && installed == latest.Tag)
                {
                    if (!silent) SetStatus("Установлена последняя версия (" + installed + ").", false);
                    UpdatePlayButton();
                    return;
                }

                if (string.IsNullOrEmpty(latest.GameSetupUrl))
                {
                    if (!silent) SetStatus("В последнем релизе нет установщика игры.", true);
                    return;
                }

                string what = string.IsNullOrEmpty(installed)
                    ? "Игра не установлена. Скачать " + latest.Tag + " (" + latest.GameSetupSizeMb + " МБ)?"
                    : "Доступна новая версия " + latest.Tag + " (установлена " + installed + "). Обновить?";
                if (MessageBox.Show(this, what, "Обновление", MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes)
                {
                    UpdatePlayButton();
                    return;
                }

                await DownloadAndRunInstallerAsync(latest);
            }
            catch (Exception ex)
            {
                if (!silent) SetStatus("Ошибка проверки обновлений: " + ex.Message, true);
            }
        }

        private async Task DownloadAndRunInstallerAsync(Release rel)
        {
            string dir = GameDir();
            Directory.CreateDirectory(dir);
            string file = Path.Combine(Path.GetTempPath(), rel.GameSetupName);

            _progress.Visible = true;
            _progress.Value = 0;
            SetStatus("Скачиваю " + rel.GameSetupName + "…", false);

            try
            {
                using (var client = new WebClient())
                {
                    client.DownloadProgressChanged += (s, e) =>
                    {
                        _progress.Value = Math.Min(100, e.ProgressPercentage);
                        SetStatus(string.Format("Скачиваю… {0:N1} / {1:N1} МБ",
                            e.BytesReceived / 1048576.0, e.TotalBytesToReceive / 1048576.0), false);
                    };
                    var tcs = new TaskCompletionSource<bool>();
                    client.DownloadFileCompleted += (s, e) =>
                    {
                        if (e.Error != null) tcs.TrySetException(e.Error);
                        else tcs.TrySetResult(true);
                    };
                    client.DownloadFileAsync(new Uri(rel.GameSetupUrl), file);
                    await tcs.Task;
                }

                SetStatus("Запускаю установщик…", false);
                Process.Start(file);
                SetStatus("Установщик открыт. Установите игру и нажмите «Играть».", false);
            }
            catch (Exception ex)
            {
                SetStatus("Не удалось скачать установщик: " + ex.Message, true);
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
            if (hasGame && !signed) _play.Text = "▶ Играть (войдите)";
            else if (hasGame) _play.Text = "▶ Играть";
            else _play.Text = "▶ Играть (не установлена)";
        }

        private void OnPlay(object sender, EventArgs e)
        {
            if (_profile == null) { SetStatus("Сначала войдите в профиль.", true); return; }
            string exe = GameExe();
            if (exe == null) { SetStatus("Игра не найдена. Нажмите «Проверить обновления».", true); return; }
            try
            {
                Process.Start(new ProcessStartInfo
                {
                    FileName = exe,
                    WorkingDirectory = Path.GetDirectoryName(exe)
                });
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

        // ---------------- Сеть и разбор ----------------

        private static async Task<string> HttpGetAsync(string url)
        {
            using (var client = new HttpClient())
            {
                client.DefaultRequestHeaders.UserAgent.ParseAdd("OpenUniverseLauncher/1.0");
                return await client.GetStringAsync(url);
            }
        }

        private sealed class Release
        {
            public string Tag;
            public string Name;
            public DateTime Date;
            public string Notes;
            public string GameSetupUrl;
            public string GameSetupName;
            public double GameSetupSizeMb;
        }

        /// <summary>Минимальный разбор ответа GitHub без сторонних библиотек.</summary>
        private static List<Release> ParseReleases(string json)
        {
            var list = new List<Release>();
            // Релизы идут объектами; свойства в JSON API имеют стабильные имена.
            var chunks = Regex.Split(json, "\\{\\s*\"url\":");
            foreach (var chunk in chunks)
            {
                var tag = Match(chunk, "\"tag_name\"\\s*:\\s*\"([^\"]+)\"");
                if (tag == null) continue;

                var rel = new Release
                {
                    Tag = tag,
                    Name = Match(chunk, "\"name\"\\s*:\\s*\"([^\"]*)\"") ?? tag,
                    Date = ParseDate(Match(chunk, "\"published_at\"\\s*:\\s*\"([^\"]+)\"")),
                    Notes = Unescape(Match(chunk, "\"body\"\\s*:\\s*\"((?:\\\\.|[^\"\\\\])*)\"") ?? "")
                };

                // Ищем установщик игры среди вложений.
                var m = Regex.Match(chunk,
                    "\"name\"\\s*:\\s*\"(OpenUniverse-Game-Setup[^\"]*\\.exe)\"[\\s\\S]*?\"browser_download_url\"\\s*:\\s*\"([^\"]+)\"[\\s\\S]*?\"size\"\\s*:\\s*(\\d+)");
                if (m.Success)
                {
                    rel.GameSetupName = m.Groups[1].Value;
                    rel.GameSetupUrl = m.Groups[2].Value;
                    rel.GameSetupSizeMb = long.Parse(m.Groups[3].Value) / 1048576.0;
                }
                list.Add(rel);
            }
            return list;
        }

        private static string Match(string s, string pattern)
        {
            var m = Regex.Match(s, pattern);
            return m.Success ? m.Groups[1].Value : null;
        }

        private static DateTime ParseDate(string iso)
        {
            DateTime d;
            if (!string.IsNullOrEmpty(iso) &&
                DateTime.TryParse(iso, null, System.Globalization.DateTimeStyles.AdjustToUniversal, out d))
                return d;
            return DateTime.MinValue;
        }

        private static string Unescape(string s)
        {
            if (string.IsNullOrEmpty(s)) return s;
            return s.Replace("\\r\\n", "\n").Replace("\\n", "\n").Replace("\\\"", "\"")
                    .Replace("\\r", "").Replace("\\t", " ").Replace("\\\\", "\\");
        }
    }
}
