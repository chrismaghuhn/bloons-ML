using System;
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Reflection;
using BloonsML.Common;
using Il2CppAssets.Scripts.Data;
using Il2CppAssets.Scripts.Models;
using Il2CppAssets.Scripts.Models.Rounds;
using Il2CppAssets.Scripts.Unity;
using MelonLoader;
using Newtonsoft.Json.Linq;
using UnityEngine;

namespace BloonsML.Dump;

/// <summary>
/// Kompletter Dump der statischen Spieldaten aus Game.instance.model und GameData.Instance (F6).
/// Läuft als Coroutine, damit das Spiel zwischendurch weiterrendert.
/// </summary>
public static class GameDataDumper
{
    /// <summary>Nach so vielen geschriebenen Objekten einen Frame abgeben.</summary>
    private const int ItemsPerFrame = 25;

    /// <summary>GameData-Felder mit eigenem Ordner (rounds/, maps/_index.json).</summary>
    private static readonly HashSet<string> GameDataSkip = new(StringComparer.OrdinalIgnoreCase)
    {
        "roundSets", "mapSet"
    };

    /// <summary>
    /// Große, verschachtelte GameData-Felder, die Mod Helper beim eigenen Export teilweise ausblendet.
    /// Kommen als letzte Daten vor summary/manifest, damit ein Problem (im schlimmsten Fall ein Absturz) möglichst
    /// wenig verhindert; dank des temporären Ordners bleibt der alte Dump dann erhalten, und das Log nennt das Feld.
    /// </summary>
    private static readonly string[] GameDataLast = { "assetLibrary", "questData", "rogueData", "frontierData", "legendsData" };

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
            run = DumpRun.Staged();
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Error(e.Message);
            yield break;
        }

        BloonsMLMod.Log.Msg($"Dump {run.RunId} startet: Spielversion {run.GameVersion} -> {run.Root}");
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

        // --- Runden-Sets: eine Datei pro Runde (mit Gruppen-Timing und, falls vorhanden, vorberechneten Emissions);
        //     jede Runde einzeln, damit eine fehlerhafte Runde nicht das ganze Set kostet
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

        // --- Restliches GameModel (nur statische Daten; Partie-Felder schreibt MapDumper nach matches/)
        foreach (var prop in GameModelFields.Static())
        {
            TryDump(run, () =>
            {
                var value = prop.GetValue(model);
                run.Write("gameModel", ModelJson.ToJTokenAny(value), "gamemodel", prop.Name + ".json");
            }, $"gameModel.{prop.Name}");
            yield return null;
        }

        // --- Restliches GameData (Income-Sets, Modus-/Schwierigkeits-Mods, Modi, …): ein File pro Feld
        foreach (var prop in GameDataProperties())
        {
            BloonsMLMod.Log.Msg($"gamedata.{prop.Name}");
            TryDump(run, () =>
            {
                var value = prop.GetValue(gameData);
                run.Write("gameData", ModelJson.ToJTokenAny(value), "gamedata", prop.Name + ".json");
            }, $"gameData.{prop.Name}");
            yield return null;
        }

        // --- Zusammenfassung + Manifest, danach temporären Ordner an die endgültige Stelle setzen
        TryDump(run, () => run.Write("summary", summary.ToJson(), "summary.json"), "summary");
        var manifest = new JObject
        {
            ["runId"] = run.RunId,
            ["gameVersion"] = run.GameVersion,
            ["unityVersion"] = Application.unityVersion,
            ["modVersion"] = typeof(BloonsMLMod).Assembly.GetName().Version?.ToString(),
            ["dumpedAtUtc"] = DateTime.UtcNow.ToString("o"),
            ["durationSeconds"] = Math.Round(sw.Elapsed.TotalSeconds, 1),
            ["written"] = JObject.FromObject(run.Written),
            ["errors"] = new JArray(run.Errors.Cast<object>().ToArray())
        };
        TryDump(run, () => run.Write("manifest", manifest, "manifest.json"), "manifest");

        try
        {
            foreach (var warning in run.Commit()) BloonsMLMod.Log.Warning(warning);
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Error($"Dump liegt in {run.Root}, konnte nicht nach {run.FinalRoot} verschoben werden: {e}");
            yield break;
        }
        BloonsMLMod.Log.Msg($"Dump fertig in {sw.Elapsed.TotalSeconds:F1}s, {run.Errors.Count} Fehler -> {run.FinalRoot}");
    }

    /// <summary>
    /// Ein Runden-Set: jede Runde einzeln (eine fehlerhafte Runde kostet nicht das Set; in der Übersicht steht
    /// für sie null, damit Index i weiter Runde i+1 ist), danach _set.json.
    /// </summary>
    private static void DumpRoundSet(DumpRun run, DumpSummary summary, RoundSetModel roundSet)
    {
        var setName = roundSet.name ?? "unnamed";
        var rounds = new JArray();
        var number = 0;
        foreach (var round in roundSet.rounds)
        {
            number++;
            try
            {
                var jo = ModelJson.ToJToken(round);
                run.Write("rounds", jo, "rounds", setName, $"{number:D3}.json");
                rounds.Add(jo);
            }
            catch (Exception e)
            {
                run.Fail($"round {setName}/{number}", e);
                rounds.Add(JValue.CreateNull());
            }
        }
        var set = new JObject
        {
            ["$type"] = roundSet.GetIl2CppType().AssemblyQualifiedName,
            ["name"] = setName,
            ["linkedIncomeSet"] = roundSet.linkedIncomeSet,
            ["roundCount"] = number
        };
        run.Write("roundSets", set, "rounds", setName, "_set.json");
        summary.AddRoundSet(setName, rounds);
    }

    /// <summary>GameData-Felder; die großen aus <see cref="GameDataLast"/> am Ende.</summary>
    private static IEnumerable<PropertyInfo> GameDataProperties() =>
        typeof(GameData).GetProperties(BindingFlags.Public | BindingFlags.Instance)
            .Where(p => p.DeclaringType == typeof(GameData) && p.CanRead && p.GetIndexParameters().Length == 0)
            .Where(p => !GameDataSkip.Contains(p.Name))
            .OrderBy(p => Array.IndexOf(GameDataLast, p.Name));

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
