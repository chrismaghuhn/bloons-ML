using System;
using System.Collections.Generic;
using System.IO;
using BloonsML.Common;
using Il2CppAssets.Scripts.Models;
using Il2CppAssets.Scripts.Models.Gameplay.Mods;
using Il2CppAssets.Scripts.Models.Map;
using Il2CppAssets.Scripts.Unity.UI_New.InGame;
using Newtonsoft.Json.Linq;

namespace BloonsML.Dump;

/// <summary>
/// Dumpt beim Start einer Partie die Geometrie der Karte (MapModel: Pfade, Platzierungsflächen, Blocker)
/// nach maps/&lt;Karte&gt;.json und die Regeln der Partie (Startgeld, Leben, Runden-/Income-Set, Mods des Modus)
/// nach matches/&lt;Spieltyp&gt;_&lt;Modus&gt;_&lt;Schwierigkeit&gt;_&lt;Karte&gt;.json.
/// Beides steckt nicht im globalen GameModel, sondern entsteht erst mit der Partie. Ablauf über Mod Helpers Hooks:
/// OnNewGameModel(…, mods) merkt sich die Mods der Partie, OnNewGameModel(…, map) die Karte (Karten-Dump),
/// LateOnNewGameModel das fertige GameModel (Match-Dump, nach Challenge-/Kosmetik-Regeln).
/// </summary>
public static class MapDumper
{
    private static GameModel lastGame;
    private static MapModel lastMap;
    private static JArray lastMods;
    private static GameModel lastModsGame;
    private static string lastMode;
    private static string lastDifficulty;
    private static string lastGameType;
    private static bool? lastReverse;

    public static void RememberMods(GameModel result, IReadOnlyList<ModModel> mods)
    {
        lastModsGame = result;
        lastMods = new JArray();
        if (mods == null) return;
        foreach (var mod in mods)
        {
            try
            {
                lastMods.Add(ModelJson.ToJToken(mod));
            }
            catch (Exception e)
            {
                lastMods.Add($"{mod?.name}: {e.GetType().Name}: {e.Message}");
            }
        }
    }

    public static void Remember(GameModel result, MapModel map)
    {
        lastMap = map;
        RememberGame(result);
    }

    /// <summary>Modus/Schwierigkeit bevorzugt aus dem GameModel der Partie, sonst aus der Menü-Auswahl.</summary>
    public static void RememberGame(GameModel result)
    {
        lastGame = result;
        lastMode = Try(() => result?.gameMode);
        lastDifficulty = Try(() => result?.difficultyId);
        lastGameType = Try(() => result?.gameType.ToString());
        lastReverse = Try<bool?>(() => result?.reverseMode);
        if (string.IsNullOrEmpty(lastMode)) lastMode = Try(() => InGameData.CurrentGame?.selectedMode);
        if (string.IsNullOrEmpty(lastDifficulty)) lastDifficulty = Try(() => InGameData.CurrentGame?.selectedDifficulty);
    }

    private static T Try<T>(Func<T> get)
    {
        try
        {
            return get();
        }
        catch (Exception)
        {
            return default;
        }
    }

    /// <param name="force">
    /// false (Auto-Dump): Karte nur schreiben, wenn sie fehlt oder bisher nur aus einer Reverse-Partie stammt.
    /// true (F7): immer.
    /// </param>
    /// <param name="includeMatch">Auch die Partie-Regeln schreiben (F7; der Auto-Dump macht das in LateOnNewGameModel).</param>
    public static void DumpLast(bool force, bool includeMatch)
    {
        if (lastMap == null)
        {
            BloonsMLMod.Log.Warning("Noch keine Karte geladen – erst eine Partie starten.");
            return;
        }
        DumpRun run;
        JObject mapJson;
        string mapName;
        try
        {
            run = DumpRun.Direct();
            mapJson = (JObject)ModelJson.ToJToken(lastMap);
            mapName = mapJson.Value<string>("mapName") ?? mapJson.Value<string>("name") ?? "unknown_map";
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Warning($"Karten-Dump fehlgeschlagen: {e}");
            return;
        }

        try
        {
            var path = run.PathFor("maps", mapName + ".json");
            if (force || !File.Exists(path) || (lastReverse == false && StoredReverse(path)))
            {
                var doc = new JObject
                {
                    ["gameVersion"] = run.GameVersion,
                    ["mapName"] = mapName,
                    ["dumpedInMode"] = lastMode,
                    ["dumpedInDifficulty"] = lastDifficulty,
                    ["reverseMode"] = lastReverse,
                    ["coordinateNote"] = "Simulationskoordinaten (siehe mod/README.md, Abschnitt Koordinaten); " +
                                         "Pfadpunkte: paths[*].points[*].point, Flächen: areas[*].polygon",
                    ["map"] = mapJson
                };
                run.Write("maps", doc, "maps", mapName + ".json");
                BloonsMLMod.Log.Msg($"Karte '{mapName}' -> {path}");
            }
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Warning($"Karten-Dump fehlgeschlagen: {e}");
        }

        if (includeMatch) DumpMatch();
    }

    /// <summary>Partie-Regeln der zuletzt erstellten Partie schreiben.</summary>
    public static void DumpMatch()
    {
        try
        {
            // Karte aus dem GameModel der Partie, falls gesetzt; sonst die zuletzt geladene
            var mapName = Try(() => lastGame?.map?.mapName) ?? Try(() => lastMap?.mapName) ?? "unknown_map";
            DumpMatch(DumpRun.Direct(), mapName);
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Warning($"Match-Dump fehlgeschlagen: {e}");
        }
    }

    private static bool StoredReverse(string path)
    {
        try
        {
            return JObject.Parse(File.ReadAllText(path)).Value<bool?>("reverseMode") == true;
        }
        catch (Exception)
        {
            return false;
        }
    }

    /// <summary>Partie-Felder des GameModels (ohne die Karte, die steht in maps/).</summary>
    private static void DumpMatch(DumpRun run, string mapName)
    {
        if (lastGame == null) return;
        var fields = new JObject();
        var errors = new JArray();
        foreach (var prop in GameModelFields.MatchOnly())
        {
            if (prop.Name is "map" or "AllMods") continue; // Karte steht in maps/, Mods kommen aus dem Hook
            try
            {
                fields[prop.Name] = ModelJson.ToJTokenAny(prop.GetValue(lastGame));
            }
            catch (Exception e)
            {
                errors.Add($"{prop.Name}: {e.GetType().Name}: {e.Message}");
            }
        }
        var mode = lastMode ?? "unknownMode";
        var difficulty = lastDifficulty ?? "unknownDifficulty";
        var doc = new JObject
        {
            ["gameVersion"] = run.GameVersion,
            ["mapName"] = mapName,
            ["gameType"] = lastGameType,
            ["mode"] = lastMode,
            ["difficulty"] = lastDifficulty,
            ["reverseMode"] = lastReverse,
            ["note"] = "GameModel-Felder dieser Partie, wie das Spiel sie beim Erstellen der Partie setzt " +
                       "(Modus, Schwierigkeit und alles, was das Profil dabei einrechnet)",
            ["fields"] = fields,
            // nur, wenn die Mods zu genau diesem GameModel gehören (sonst stammen sie aus einer früheren Partie)
            ["mods"] = ReferenceEquals(lastModsGame, lastGame) || lastModsGame?.Pointer == lastGame?.Pointer ? lastMods : null,
            ["errors"] = errors
        };
        var file = $"{lastGameType ?? "unknownType"}_{mode}_{difficulty}_{mapName}.json";
        run.Write("matches", doc, "matches", file);
        BloonsMLMod.Log.Msg($"Match {lastGameType}/{mode}/{difficulty} auf '{mapName}' -> {run.PathFor("matches", file)}");
    }
}
