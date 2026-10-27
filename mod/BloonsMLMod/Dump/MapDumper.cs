using System;
using BloonsML.Common;
using Il2CppAssets.Scripts.Models;
using Il2CppAssets.Scripts.Models.Map;
using Newtonsoft.Json.Linq;

namespace BloonsML.Dump;

/// <summary>
/// Dumpt die Geometrie der gerade geladenen Karte (MapModel: Pfade, Platzierungsflächen, Blocker).
/// Karten-Geometrie steckt nicht im globalen GameModel, sondern wird erst beim Starten einer Partie
/// geladen; deshalb hängt der Dump an Mod Helpers OnNewGameModel-Hook.
/// </summary>
public static class MapDumper
{
    private static MapModel lastMap;
    private static string lastDifficulty;

    public static void Remember(GameModel result, MapModel map)
    {
        lastMap = map;
        try
        {
            lastDifficulty = result?.difficultyId;
        }
        catch (Exception)
        {
            lastDifficulty = null;
        }
    }

    public static void DumpLast()
    {
        if (lastMap == null)
        {
            BloonsMLMod.Log.Warning("Noch keine Karte geladen – erst eine Partie starten.");
            return;
        }
        try
        {
            var run = DumpRun.ForCurrentGame();
            var mapJson = (JObject)ModelJson.ToJToken(lastMap);
            var mapName = mapJson.Value<string>("mapName") ?? mapJson.Value<string>("name") ?? "unknown_map";
            var doc = new JObject
            {
                ["gameVersion"] = run.GameVersion,
                ["mapName"] = mapName,
                ["difficultyAtDump"] = lastDifficulty,
                ["coordinateNote"] = "Simulationskoordinaten des Spiels (gleiches System wie Turm-Positionen/Reichweiten); " +
                                     "Pfadpunkte: paths[*].points[*].point, Flächen: areas[*].polygon",
                ["map"] = mapJson
            };
            run.Write("maps", doc, "maps", mapName + ".json");
            BloonsMLMod.Log.Msg($"Karte '{mapName}' -> {run.PathFor("maps", mapName + ".json")}");
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Warning($"Karten-Dump fehlgeschlagen: {e}");
        }
    }
}
