using System;
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Reflection;
using BloonsML.Common;
using Il2CppAssets.Scripts.Data;
using Il2CppAssets.Scripts.Models;
using Il2CppAssets.Scripts.Unity;
using MelonLoader;
using Newtonsoft.Json.Linq;
using UnityEngine;
using Il2CppObject = Il2CppSystem.Object;

namespace BloonsML.Dump;

/// <summary>
/// Kompletter Dump der statischen Spieldaten aus Game.instance.model und GameData.Instance.
/// Läuft als Coroutine, damit das Spiel zwischendurch weiterrendert.
/// </summary>
public static class GameDataDumper
{
    /// <summary>Nach so vielen geschriebenen Objekten einen Frame abgeben.</summary>
    private const int ItemsPerFrame = 25;

    /// <summary>GameModel-Felder, die eigene Ordner bekommen oder nur Laufzeit-Caches sind.</summary>
    private static readonly HashSet<string> GameModelSkip = new(StringComparer.OrdinalIgnoreCase)
    {
        "towers", "upgrades", "bloons", "searchCache", "childDependants"
    };

    public static bool Running { get; private set; }

    public static void Start()
    {
        if (Running)
        {
            BloonsMLMod.Log.Warning("Dump läuft bereits.");
            return;
        }
        if (Game.instance?.model == null || GameData.Instance == null)
        {
            BloonsMLMod.Log.Warning("Spieldaten noch nicht geladen – im Hauptmenü erneut versuchen.");
            return;
        }
        Running = true;
        MelonCoroutines.Start(Guarded(Run()));
    }

    /// <summary>Unerwartete Exceptions in der Coroutine loggen und Running zurücksetzen.</summary>
    private static IEnumerator Guarded(IEnumerator inner)
    {
        while (true)
        {
            object current;
            try
            {
                if (!inner.MoveNext()) break;
                current = inner.Current;
            }
            catch (Exception e)
            {
                BloonsMLMod.Log.Error($"Dump abgebrochen: {e}");
                break;
            }
            yield return current;
        }
        Running = false;
    }

    private static IEnumerator Run()
    {
        var sw = Stopwatch.StartNew();
        DumpRun run;
        try
        {
            run = DumpRun.ForCurrentGame();
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Error(e.Message);
            yield break;
        }

        BloonsMLMod.Log.Msg($"Dump startet: Spielversion {run.GameVersion} -> {run.Root}");
        var model = Game.instance.model;
        var gameData = GameData.Instance;
        var summary = new DumpSummary(run.GameVersion);

        // --- Türme: jede TowerModel-Instanz = eine gültige Upgrade-Kombination (inkl. Crosspaths, Helden-Level, Paragons, Sub-Towers)
        var n = 0;
        foreach (var tower in model.towers)
        {
            TryDump(run, () =>
            {
                var jo = (JObject)ModelJson.ToJToken(tower);
                run.Write("towers", jo, "towers", tower.baseId, tower.name + ".json");
                summary.AddTower(jo);
            }, $"tower {tower?.name}");
            if (++n % ItemsPerFrame == 0) yield return null;
        }
        BloonsMLMod.Log.Msg($"Türme: {Count(run, "towers")}");

        // --- Upgrades (Kosten, Pfad, Tier)
        foreach (var upgrade in model.upgrades)
        {
            TryDump(run, () =>
            {
                var jo = (JObject)ModelJson.ToJToken(upgrade);
                run.Write("upgrades", jo, "upgrades", upgrade.name + ".json");
                summary.AddUpgrade(jo);
            }, $"upgrade {upgrade?.name}");
            if (++n % ItemsPerFrame == 0) yield return null;
        }
        BloonsMLMod.Log.Msg($"Upgrades: {Count(run, "upgrades")}");

        // --- Bloons
        foreach (var bloon in model.bloons)
        {
            TryDump(run, () =>
            {
                var jo = (JObject)ModelJson.ToJToken(bloon);
                run.Write("bloons", jo, "bloons", bloon.baseId, bloon.name + ".json");
                summary.AddBloon(jo);
            }, $"bloon {bloon?.name}");
            if (++n % ItemsPerFrame == 0) yield return null;
        }
        BloonsMLMod.Log.Msg($"Bloons: {Count(run, "bloons")}");

        // --- Runden-Sets: eine Datei pro Runde (mit Gruppen-Timing und, falls vorhanden, vorberechneten Emissions)
        foreach (var roundSet in gameData.roundSets)
        {
            TryDump(run, () => DumpRoundSet(run, summary, roundSet), $"roundSet {roundSet?.name}");
            yield return null;
        }
        BloonsMLMod.Log.Msg($"Runden: {Count(run, "rounds")} in {Count(run, "roundSets")} Sets");

        // --- Karten-Index (Metadaten aller Karten; Geometrie kommt aus MapDumper beim Laden einer Karte)
        TryDump(run, () =>
        {
            var maps = new JArray();
            foreach (var map in gameData.mapSet.Maps.items) maps.Add(ModelJson.ToJToken(map));
            run.Write("mapIndex", maps, "maps", "_index.json");
            summary.SetMapIndex(maps);
        }, "map index");
        yield return null;

        // --- Restliches GameModel: jedes Feld einzeln, damit ein kaputtes Feld nicht alles blockiert
        foreach (var prop in GameModelProperties())
        {
            TryDump(run, () =>
            {
                var value = prop.GetValue(model);
                run.Write("gameModel", ModelJson.ToJTokenAny(value), "gamemodel", prop.Name + ".json");
            }, $"gameModel.{prop.Name}");
            yield return null;
        }

        // --- Zusammenfassung + Manifest
        TryDump(run, () => run.Write("summary", summary.ToJson(), "summary.json"), "summary");
        var manifest = new JObject
        {
            ["gameVersion"] = run.GameVersion,
            ["unityVersion"] = Application.unityVersion,
            ["modVersion"] = typeof(BloonsMLMod).Assembly.GetName().Version?.ToString(),
            ["dumpedAtUtc"] = DateTime.UtcNow.ToString("o"),
            ["durationSeconds"] = Math.Round(sw.Elapsed.TotalSeconds, 1),
            ["written"] = JObject.FromObject(run.Written),
            ["errors"] = new JArray(run.Errors.Cast<object>().ToArray())
        };
        TryDump(run, () => run.Write("manifest", manifest, "manifest.json"), "manifest");

        BloonsMLMod.Log.Msg($"Dump fertig in {sw.Elapsed.TotalSeconds:F1}s, {run.Errors.Count} Fehler -> {run.Root}");
    }

    private static void DumpRoundSet(DumpRun run, DumpSummary summary, Il2CppObject roundSet)
    {
        var jo = (JObject)ModelJson.ToJToken(roundSet);
        var name = jo.Value<string>("name") ?? "unnamed";
        var rounds = jo["rounds"] as JArray ?? new JArray();
        jo.Remove("rounds");
        jo["roundCount"] = rounds.Count;
        run.Write("roundSets", jo, "rounds", name, "_set.json");
        for (var i = 0; i < rounds.Count; i++)
        {
            run.Write("rounds", rounds[i], "rounds", name, $"{i + 1:D3}.json");
        }
        summary.AddRoundSet(name, rounds);
    }

    private static IEnumerable<PropertyInfo> GameModelProperties() =>
        typeof(GameModel).GetProperties(BindingFlags.Public | BindingFlags.Instance)
            .Where(p => p.DeclaringType == typeof(GameModel) && p.CanRead && p.GetIndexParameters().Length == 0)
            .Where(p => !GameModelSkip.Contains(p.Name));

    private static void TryDump(DumpRun run, Action action, string what)
    {
        try
        {
            action();
        }
        catch (Exception e)
        {
            run.Fail(what, e);
        }
    }

    private static int Count(DumpRun run, string category) =>
        run.Written.TryGetValue(category, out var c) ? c : 0;
}
