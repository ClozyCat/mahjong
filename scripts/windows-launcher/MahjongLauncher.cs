using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Net;
using System.Net.Sockets;
using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Windows.Forms;

namespace MahjongLauncher
{
    internal static class Program
    {
        private const string PayloadResourceName = "MahjongLauncher.Payload.zip";
        private const string AppDataFolderName = "Mahjong";
        private const string StateFileName = "launcher-state.txt";
        private const string BackendRelativePath = "backend\\mahjong-backend.exe";
        private const string FrontendRelativePath = "web\\index.html";
        private static readonly int[] CandidatePorts = Enumerable.Range(58080, 10).ToArray();

        [STAThread]
        private static int Main()
        {
            try
            {
                var assembly = Assembly.GetExecutingAssembly();
                var payloadHash = ComputePayloadHash(assembly);
                var appRoot = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                    AppDataFolderName
                );
                var runtimeRoot = Path.Combine(appRoot, "runtime", payloadHash);
                var dataRoot = Path.Combine(appRoot, "data");
                var statePath = Path.Combine(appRoot, StateFileName);

                Directory.CreateDirectory(appRoot);
                Directory.CreateDirectory(dataRoot);
                EnsurePayloadExtracted(assembly, runtimeRoot, payloadHash);

                var state = LoadState(statePath);
                if (CanReuseExistingServer(state, payloadHash))
                {
                    OpenBrowser(BuildBaseUrl(state.Port));
                    return 0;
                }

                StopPreviousServer(state);

                var port = FindAvailablePort();
                var process = StartBackend(runtimeRoot, dataRoot, port);
                WaitForHealthy(port, TimeSpan.FromSeconds(15));
                SaveState(statePath, new LauncherState
                {
                    PayloadHash = payloadHash,
                    Port = port,
                    ProcessId = process.Id,
                });
                OpenBrowser(BuildBaseUrl(port));
                return 0;
            }
            catch (Exception error)
            {
                MessageBox.Show(
                    error.Message,
                    "Mahjong Launcher",
                    MessageBoxButtons.OK,
                    MessageBoxIcon.Error
                );
                return 1;
            }
        }

        private static string BuildBaseUrl(int port)
        {
            return string.Format("http://127.0.0.1:{0}/", port);
        }

        private static string ComputePayloadHash(Assembly assembly)
        {
            using (var stream = OpenPayloadStream(assembly))
            using (var sha256 = SHA256.Create())
            {
                var hash = sha256.ComputeHash(stream);
                var builder = new StringBuilder(hash.Length * 2);
                foreach (var item in hash)
                {
                    builder.Append(item.ToString("x2"));
                }

                return builder.ToString().Substring(0, 16);
            }
        }

        private static Stream OpenPayloadStream(Assembly assembly)
        {
            var stream = assembly.GetManifestResourceStream(PayloadResourceName);
            if (stream == null)
            {
                throw new InvalidOperationException("Embedded payload.zip was not found in the launcher.");
            }

            return stream;
        }

        private static void EnsurePayloadExtracted(Assembly assembly, string runtimeRoot, string payloadHash)
        {
            var markerPath = Path.Combine(runtimeRoot, ".payload-hash");
            var backendPath = Path.Combine(runtimeRoot, BackendRelativePath);
            var frontendIndexPath = Path.Combine(runtimeRoot, FrontendRelativePath);
            if (
                File.Exists(markerPath) &&
                string.Equals(File.ReadAllText(markerPath).Trim(), payloadHash, StringComparison.Ordinal) &&
                File.Exists(backendPath) &&
                File.Exists(frontendIndexPath)
            )
            {
                return;
            }

            if (Directory.Exists(runtimeRoot))
            {
                Directory.Delete(runtimeRoot, true);
            }

            Directory.CreateDirectory(runtimeRoot);
            var runtimeRootFullPath = Path.GetFullPath(runtimeRoot);

            using (var stream = OpenPayloadStream(assembly))
            using (var archive = new ZipArchive(stream, ZipArchiveMode.Read))
            {
                foreach (var entry in archive.Entries)
                {
                    var destinationPath = Path.GetFullPath(Path.Combine(runtimeRoot, entry.FullName));
                    if (!destinationPath.StartsWith(runtimeRootFullPath, StringComparison.OrdinalIgnoreCase))
                    {
                        throw new InvalidOperationException("Payload contains an invalid file path.");
                    }

                    if (string.IsNullOrEmpty(entry.Name))
                    {
                        Directory.CreateDirectory(destinationPath);
                        continue;
                    }

                    var parentDirectory = Path.GetDirectoryName(destinationPath);
                    if (!string.IsNullOrEmpty(parentDirectory))
                    {
                        Directory.CreateDirectory(parentDirectory);
                    }

                    using (var input = entry.Open())
                    using (var output = File.Create(destinationPath))
                    {
                        input.CopyTo(output);
                    }
                }
            }

            File.WriteAllText(markerPath, payloadHash);
        }

        private static LauncherState LoadState(string statePath)
        {
            if (!File.Exists(statePath))
            {
                return null;
            }

            var values = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (var line in File.ReadAllLines(statePath))
            {
                var separatorIndex = line.IndexOf('=');
                if (separatorIndex <= 0)
                {
                    continue;
                }

                var key = line.Substring(0, separatorIndex).Trim();
                var value = line.Substring(separatorIndex + 1).Trim();
                if (key.Length == 0)
                {
                    continue;
                }

                values[key] = value;
            }

            int port;
            int processId;
            if (
                !values.ContainsKey("payload_hash") ||
                !values.ContainsKey("port") ||
                !values.ContainsKey("process_id") ||
                !int.TryParse(values["port"], out port) ||
                !int.TryParse(values["process_id"], out processId)
            )
            {
                return null;
            }

            return new LauncherState
            {
                PayloadHash = values["payload_hash"],
                Port = port,
                ProcessId = processId,
            };
        }

        private static void SaveState(string statePath, LauncherState state)
        {
            var content = string.Join(
                Environment.NewLine,
                new[]
                {
                    "payload_hash=" + state.PayloadHash,
                    "port=" + state.Port,
                    "process_id=" + state.ProcessId,
                }
            );
            File.WriteAllText(statePath, content + Environment.NewLine);
        }

        private static bool CanReuseExistingServer(LauncherState state, string payloadHash)
        {
            if (state == null)
            {
                return false;
            }

            if (!string.Equals(state.PayloadHash, payloadHash, StringComparison.Ordinal))
            {
                return false;
            }

            if (!IsProcessAlive(state.ProcessId))
            {
                return false;
            }

            return IsHealthy(state.Port, 1000);
        }

        private static void StopPreviousServer(LauncherState state)
        {
            if (state == null || state.ProcessId <= 0)
            {
                return;
            }

            try
            {
                var process = Process.GetProcessById(state.ProcessId);
                if (process.HasExited)
                {
                    return;
                }

                process.Kill();
                process.WaitForExit(5000);
            }
            catch
            {
            }
        }

        private static bool IsProcessAlive(int processId)
        {
            try
            {
                var process = Process.GetProcessById(processId);
                return !process.HasExited;
            }
            catch
            {
                return false;
            }
        }

        private static int FindAvailablePort()
        {
            foreach (var port in CandidatePorts)
            {
                if (IsPortAvailable(port))
                {
                    return port;
                }
            }

            throw new InvalidOperationException(
                "Unable to find an available local port in the 58080-58089 range."
            );
        }

        private static bool IsPortAvailable(int port)
        {
            TcpListener listener = null;
            try
            {
                listener = new TcpListener(IPAddress.Loopback, port);
                listener.Start();
                return true;
            }
            catch
            {
                return false;
            }
            finally
            {
                if (listener != null)
                {
                    listener.Stop();
                }
            }
        }

        private static Process StartBackend(string runtimeRoot, string dataRoot, int port)
        {
            var backendPath = Path.Combine(runtimeRoot, BackendRelativePath);
            var frontendPath = Path.Combine(runtimeRoot, "web");
            if (!File.Exists(backendPath))
            {
                throw new FileNotFoundException("Bundled backend executable was not found.", backendPath);
            }

            if (!Directory.Exists(frontendPath))
            {
                throw new DirectoryNotFoundException("Bundled frontend files were not found.");
            }

            var processInfo = new ProcessStartInfo
            {
                FileName = backendPath,
                WorkingDirectory = Path.GetDirectoryName(backendPath),
                UseShellExecute = false,
                CreateNoWindow = true,
                WindowStyle = ProcessWindowStyle.Hidden,
            };
            processInfo.EnvironmentVariables["MAHJONG_BIND_ADDR"] = "127.0.0.1:" + port;
            processInfo.EnvironmentVariables["MAHJONG_DATABASE_URL"] = Path.Combine(dataRoot, "mahjong.db");
            processInfo.EnvironmentVariables["MAHJONG_FRONTEND_DIR"] = frontendPath;

            var process = Process.Start(processInfo);
            if (process == null)
            {
                throw new InvalidOperationException("Failed to start the bundled backend process.");
            }

            return process;
        }

        private static void WaitForHealthy(int port, TimeSpan timeout)
        {
            var deadline = DateTime.UtcNow.Add(timeout);
            while (DateTime.UtcNow < deadline)
            {
                if (IsHealthy(port, 1000))
                {
                    return;
                }

                Thread.Sleep(250);
            }

            throw new TimeoutException("The local Mahjong server did not become healthy in time.");
        }

        private static bool IsHealthy(int port, int timeoutMs)
        {
            try
            {
                var request = (HttpWebRequest)WebRequest.Create(BuildBaseUrl(port) + "api/health");
                request.Method = "GET";
                request.Timeout = timeoutMs;
                request.ReadWriteTimeout = timeoutMs;

                using (var response = (HttpWebResponse)request.GetResponse())
                using (var stream = response.GetResponseStream())
                using (var reader = new StreamReader(stream))
                {
                    if (response.StatusCode != HttpStatusCode.OK)
                    {
                        return false;
                    }

                    var body = reader.ReadToEnd();
                    return body.IndexOf("\"status\":\"ok\"", StringComparison.OrdinalIgnoreCase) >= 0;
                }
            }
            catch
            {
                return false;
            }
        }

        private static void OpenBrowser(string url)
        {
            var processInfo = new ProcessStartInfo
            {
                FileName = url,
                UseShellExecute = true,
            };
            Process.Start(processInfo);
        }

        private sealed class LauncherState
        {
            public string PayloadHash { get; set; }
            public int Port { get; set; }
            public int ProcessId { get; set; }
        }
    }
}
