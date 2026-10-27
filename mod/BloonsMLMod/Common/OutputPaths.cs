using System;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;

namespace BloonsML.Common;

/// <summary>
/// Ausgabepfade der Mod. Das Spiel läuft unter Proton/Wine, Konfiguration erfolgt aber mit
/// Unix-Pfaden (/home/...); die werden hier auf das Wine-Laufwerk Z:\ abgebildet.
/// Gemeinsam genutzt von Dump und (später) Recorder/Bridge.
/// </summary>
public static class OutputPaths
{
    /// <summary>Beim Build eingebackenes Standard-Dumpziel (siehe btd6-paths.props).</summary>
    public static string DefaultDumpRoot =>
        typeof(OutputPaths).Assembly.GetCustomAttributes<AssemblyMetadataAttribute>()
            .FirstOrDefault(a => a.Key == "DefaultDumpRoot")?.Value ?? "";

    /// <summary>Unix-Pfad unter Wine nach Z:\... umschreiben; Windows-Pfade bleiben unverändert.</summary>
    public static string ToHostPath(string path)
    {
        if (string.IsNullOrWhiteSpace(path)) return path;
        path = path.Trim();
        if (path.StartsWith("~/"))
        {
            var home = Environment.GetEnvironmentVariable("HOME");
            if (!string.IsNullOrEmpty(home)) path = home.TrimEnd('/') + path[1..];
        }
        if (OperatingSystem.IsWindows() && path.StartsWith("/"))
        {
            return "Z:" + path.Replace('/', '\\');
        }
        return path;
    }

    /// <summary>Dateinamen-taugliche Variante eines Spiel-Namens (Upgrades enthalten z.B. Leerzeichen, ':' oder '/').</summary>
    public static string Sanitize(string name)
    {
        if (string.IsNullOrEmpty(name)) return "_unnamed";
        var invalid = Path.GetInvalidFileNameChars().Concat(new[] { '/', '\\', ':', '*', '?', '"', '<', '>', '|' }).ToHashSet();
        var sb = new StringBuilder(name.Length);
        foreach (var c in name) sb.Append(invalid.Contains(c) ? '_' : c);
        return sb.ToString().Trim().TrimEnd('.');
    }
}
