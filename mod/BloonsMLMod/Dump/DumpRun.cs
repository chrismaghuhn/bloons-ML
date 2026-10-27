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
        return new DumpRun(version, Path.Combine(baseDir, OutputPaths.Sanitize(version)));
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
        ModelJson.Write(PathFor(parts), token);
        Written[category] = Written.TryGetValue(category, out var n) ? n + 1 : 1;
    }

    public void Fail(string what, Exception e)
    {
        var msg = $"{what}: {e.GetType().Name}: {e.Message}";
        Errors.Add(msg);
        BloonsMLMod.Log.Warning(msg);
    }
}
