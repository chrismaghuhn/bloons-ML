using System;
using System.Collections.Generic;
using System.IO;
using BloonsML.Common;
using Newtonsoft.Json.Linq;
using UnityEngine;

namespace BloonsML.Dump;

/// <summary>Zustand eines Dump-Laufs: Zielordner, Zähler, Fehler.</summary>
public sealed class DumpRun
{
    public string GameVersion { get; }
    public string Root { get; }
    public Dictionary<string, int> Written { get; } = new();
    public List<string> Errors { get; } = new();

    // Wine vergleicht Dateinamen ohne Groß-/Kleinschreibung; Kollisionen sonst stilles Überschreiben
    private readonly HashSet<string> writtenPaths = new(StringComparer.OrdinalIgnoreCase);

    private DumpRun(string gameVersion, string root)
    {
        GameVersion = gameVersion;
        Root = root;
    }

    /// <summary>&lt;DumpRoot&gt;/&lt;Spielversion&gt;</summary>
    public static DumpRun ForCurrentGame()
    {
        var version = Application.version;
        var baseDir = OutputPaths.ToHostPath(BloonsMLMod.DumpRoot);
        if (string.IsNullOrWhiteSpace(baseDir))
            throw new InvalidOperationException("Kein Dump-Ordner konfiguriert (Mod-Einstellung 'Dump-Ordner').");
        ProtectFromGit(baseDir);
        return new DumpRun(version, Path.Combine(baseDir, OutputPaths.Sanitize(version)));
    }

    /// <summary>
    /// Legt im Dump-Ordner eine .gitignore mit "*" an: Die Daten gehören Ninja Kiwi und dürfen nicht
    /// ins (öffentliche) Repo, auch wenn der Ordner in einem Checkout ohne passende .gitignore liegt.
    /// </summary>
    private static void ProtectFromGit(string baseDir)
    {
        Directory.CreateDirectory(baseDir);
        var gitignore = Path.Combine(baseDir, ".gitignore");
        if (!File.Exists(gitignore)) File.WriteAllText(gitignore, "*\n");
    }

    public string PathFor(params string[] parts)
    {
        var segments = new string[parts.Length + 1];
        segments[0] = Root;
        for (var i = 0; i < parts.Length; i++) segments[i + 1] = OutputPaths.Sanitize(parts[i]);
        return Path.Combine(segments);
    }

    public void Write(string category, JToken token, params string[] parts)
    {
        var path = PathFor(parts);
        if (!writtenPaths.Add(path))
            throw new IOException($"Dateiname kollidiert mit bereits geschriebener Datei: {path}");
        ModelJson.Write(path, token);
        Written[category] = Written.TryGetValue(category, out var n) ? n + 1 : 1;
    }

    public void Fail(string what, Exception e)
    {
        var msg = $"{what}: {e.GetType().Name}: {e.Message}";
        Errors.Add(msg);
        BloonsMLMod.Log.Warning(msg);
    }
}
