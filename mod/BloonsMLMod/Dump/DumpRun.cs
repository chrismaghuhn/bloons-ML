using System;
using System.Collections.Generic;
using System.IO;
using BloonsML.Common;
using Newtonsoft.Json.Linq;
using UnityEngine;

namespace BloonsML.Dump;

/// <summary>
/// Zustand eines Dump-Laufs: Zielordner, Zähler, Fehler.
/// Ein kompletter Dump (F6) schreibt in &lt;version&gt;.tmp-&lt;runId&gt;/ und ersetzt &lt;version&gt;/ erst am Ende
/// (<see cref="Commit"/>); ein abgebrochener Dump lässt den alten Stand unverändert. Karten- und Match-Dateien
/// (maps/&lt;Karte&gt;.json, matches/) entstehen nur beim Spielen und werden in den neuen Ordner übernommen.
/// </summary>
public sealed class DumpRun
{
    /// <summary>Ordner, die nur MapDumper (beim Start einer Partie) schreibt und F6 nicht neu erzeugt.</summary>
    private static readonly string[] PlayDirs = { "maps", "matches" };

    public string GameVersion { get; }
    public string RunId { get; }
    public string Root { get; }
    public string FinalRoot { get; }
    public Dictionary<string, int> Written { get; } = new();
    public List<string> Errors { get; } = new();

    // Wine vergleicht Dateinamen ohne Groß-/Kleinschreibung; Kollisionen sonst stilles Überschreiben
    private readonly HashSet<string> writtenPaths = new(StringComparer.OrdinalIgnoreCase);

    private DumpRun(string gameVersion, string runId, string root, string finalRoot)
    {
        GameVersion = gameVersion;
        RunId = runId;
        Root = root;
        FinalRoot = finalRoot;
    }

    /// <summary>Schreibt direkt nach &lt;DumpRoot&gt;/&lt;Spielversion&gt; (für einzelne Karten/Matches).</summary>
    public static DumpRun Direct()
    {
        var (version, finalRoot) = Locate();
        return new DumpRun(version, NewRunId(), finalRoot, finalRoot);
    }

    /// <summary>Kompletter Dump in einen temporären Ordner; erst <see cref="Commit"/> macht ihn sichtbar.</summary>
    public static DumpRun Staged()
    {
        var (version, finalRoot) = Locate();
        var runId = NewRunId();
        RemoveStale(finalRoot);
        return new DumpRun(version, runId, $"{finalRoot}.tmp-{runId}", finalRoot);
    }

    /// <summary>Temporären Ordner an die endgültige Stelle setzen; Karten/Matches aus dem alten Stand übernehmen.</summary>
    public void Commit()
    {
        if (Root == FinalRoot) return;
        if (Directory.Exists(FinalRoot))
        {
            foreach (var dir in PlayDirs)
            {
                var oldDir = Path.Combine(FinalRoot, dir);
                if (!Directory.Exists(oldDir)) continue;
                foreach (var file in Directory.GetFiles(oldDir))
                {
                    var target = Path.Combine(Root, dir, Path.GetFileName(file));
                    if (File.Exists(target)) continue; // frisch geschrieben (z.B. maps/_index.json) gewinnt
                    Directory.CreateDirectory(Path.GetDirectoryName(target)!);
                    File.Move(file, target);
                }
            }
            var old = $"{FinalRoot}.old-{RunId}";
            Directory.Move(FinalRoot, old);
            try
            {
                Directory.Move(Root, FinalRoot);
            }
            catch
            {
                Directory.Move(old, FinalRoot); // alten Stand wiederherstellen
                throw;
            }
            Directory.Delete(old, true);
        }
        else
        {
            Directory.Move(Root, FinalRoot);
        }
    }

    private static (string version, string finalRoot) Locate()
    {
        var version = Application.version;
        var baseDir = OutputPaths.ToHostPath(BloonsMLMod.DumpRoot);
        if (string.IsNullOrWhiteSpace(baseDir))
            throw new InvalidOperationException("Kein Dump-Ordner konfiguriert (Mod-Einstellung 'Dump-Ordner').");
        ProtectFromGit(baseDir);
        return (version, Path.Combine(baseDir, OutputPaths.Sanitize(version)));
    }

    private static string NewRunId() => DateTime.UtcNow.ToString("yyyyMMdd'T'HHmmss'Z'");

    /// <summary>Reste abgebrochener Läufe (.tmp-*/.old-*) derselben Version entfernen.</summary>
    private static void RemoveStale(string finalRoot)
    {
        var parent = Path.GetDirectoryName(finalRoot)!;
        var name = Path.GetFileName(finalRoot);
        foreach (var dir in Directory.GetDirectories(parent, name + ".*"))
        {
            var suffix = Path.GetFileName(dir)[name.Length..];
            if (!suffix.StartsWith(".tmp-") && !suffix.StartsWith(".old-")) continue;
            try
            {
                Directory.Delete(dir, true);
            }
            catch (Exception e)
            {
                BloonsMLMod.Log.Warning($"Konnte alten Dump-Rest nicht löschen: {dir}: {e.Message}");
            }
        }
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
