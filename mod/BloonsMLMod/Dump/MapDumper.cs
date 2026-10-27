using System;
using System.IO;
using BloonsML.Common;
using Il2CppAssets.Scripts.Models;
using Il2CppAssets.Scripts.Models.Map;
using Il2CppAssets.Scripts.Unity.UI_New.InGame;
using Newtonsoft.Json.Linq;

namespace BloonsML.Dump;

/// <summary>
/// Dumpt beim Start einer Partie die Geometrie der Karte (MapModel: Pfade, Platzierungsflächen, Blocker)
/// nach maps/&lt;Karte&gt;.json und die Regeln der Partie (Startgeld, Leben, Runden-/Income-Set, Mods des Modus)
/// nach matches/&lt;Modus&gt;_&lt;Schwierigkeit&gt;_&lt;Karte&gt;.json.
/// Beides steckt nicht im globalen GameModel, sondern entsteht erst mit der Partie; deshalb hängt der Dump an
/// Mod Helpers OnNewGameModel-Hook.
/// </summary>
public static class MapDumper
{
    private static GameModel lastGame;
    private static MapModel lastMap;
    private static string lastMode;
    private static string lastDifficulty;
    private static bool? lastReverse;

    public static void Remember(GameModel result, MapModel map)
    {
        lastGame = result;
        lastMap = map;
        try
        {
            var current = InGameData.CurrentGame;
            lastMode = current?.selectedMode;
            lastDifficulty = current?.selectedDifficulty;
        }
        catch (Exception)
        {
            lastMode = lastDifficulty = null;
        }
        try
        {
            lastReverse = result?.reverseMode;
        }
        catch (Exception)
        {
            lastReverse = null;
        }
    }

    /// <param name="force">false (Auto-Dump): Karte nur schreiben, wenn sie noch fehlt. true (F7): immer.</param>
    public static void DumpLast(bool force)
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
            if (force || !File.Exists(path))
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

        try
        {
            DumpMatch(run, mapName);
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Warning($"Match-Dump fehlgeschlagen: {e}");
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
            if (prop.Name == "map") continue;
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
            ["mode"] = lastMode,
            ["difficulty"] = lastDifficulty,
            ["note"] = "GameModel-Felder dieser Partie, wie das Spiel sie beim Erstellen der Partie setzt " +
                       "(Modus, Schwierigkeit und alles, was das Profil dabei einrechnet)",
            ["fields"] = fields,
            ["errors"] = errors
        };
        var file = $"{mode}_{difficulty}_{mapName}.json";
        run.Write("matches", doc, "matches", file);
        BloonsMLMod.Log.Msg($"Match {mode}/{difficulty} auf '{mapName}' -> {run.PathFor("matches", file)}");
    }
}
