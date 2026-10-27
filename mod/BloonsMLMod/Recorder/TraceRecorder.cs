using System;
using System.Globalization;
using System.IO;
using System.Text;
using BloonsML.Common;
using HarmonyLib;
using Il2CppAssets.Scripts.Simulation;
using Il2CppAssets.Scripts.Simulation.Bloons;
using Il2CppAssets.Scripts.Simulation.Towers;
using Il2CppAssets.Scripts.Simulation.Towers.Weapons;
using Il2CppAssets.Scripts.Unity.UI_New.InGame;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEngine;

namespace BloonsML.Recorder;

/// <summary>
/// Test-Recorder (Schritt 1 aus docs/design-recorder.md): schreibt pro Sim-Schritt den internen Zähler und die
/// Bloon-Positionen sowie Spawn/Leak/Runde/Turm/Schuss als JSONL nach
/// &lt;DumpRoot&gt;/../recordings/&lt;Zeit&gt;_&lt;Karte&gt;_&lt;Modus&gt;_&lt;Schwierigkeit&gt;.trace.jsonl.gz.
/// Zweck: Hooks/Felder prüfen, Zeitbasis (1× gegen vorgespult), Spawn-/Leak-Distanz. Format siehe mod/README.md.
/// </summary>
public static class TraceRecorder
{
    private const string FormatVersion = "bloonsml-trace/0";
    private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

    private static TraceWriter writer;
    private static int lastUnityFrame = -1;
    private static int stepsThisFrame;
    private static bool lastFastForward;

    /// <summary>Bloons, deren Tempo-Werte schon geschrieben wurden (beim Erzeugen sind sie noch nicht gesetzt).</summary>
    private static readonly System.Collections.Generic.HashSet<uint> describedBloons = new();

    public static bool Active => writer != null;

    /// <summary>Einstellung/Hotkey geändert: im Spiel sofort starten bzw. stoppen.</summary>
    public static void SetEnabled(bool enabled)
    {
        if (!enabled) Stop("ausgeschaltet");
        BloonsMLMod.Log.Msg($"Trace {(enabled ? "an (startet mit dem nächsten Sim-Schritt)" : "aus")}");
    }

    public static void Stop(string reason)
    {
        if (writer == null) return;
        Emit(new StringBuilder("{\"ev\":\"stop\",\"reason\":").Append(JsonConvert.ToString(reason)).Append('}'));
        var w = writer;
        writer = null;
        w.Dispose();
        BloonsMLMod.Log.Msg($"Trace geschlossen ({reason}): {w.Lines} Zeilen -> {w.Path}");
    }

    // ------------------------------------------------------------------ Sim-Schritt

    /// <summary>Nach jedem Simulation.Simulate (ein Sim-Schritt).</summary>
    internal static void OnSimulate(Simulation sim)
    {
        if (!BloonsMLMod.RecordTrace) return;
        if (writer == null && !TryStart(sim)) return;

        var frame = Time.frameCount;
        if (frame != lastUnityFrame)
        {
            lastUnityFrame = frame;
            stepsThisFrame = 0;
        }
        stepsThisFrame++;

        var ff = FastForward();
        if (ff != lastFastForward)
        {
            lastFastForward = ff;
            Emit(Event("fastforward").Append(",\"on\":").Append(ff ? "true" : "false").Append('}'));
        }

        var sb = new StringBuilder(256);
        sb.Append("{\"ev\":\"step\",\"t\":").Append(sim.time.elapsed)
            .Append(",\"rt\":").Append(sim.roundTime.elapsed)
            .Append(",\"uf\":").Append(frame)
            .Append(",\"sif\":").Append(stepsThisFrame)
            .Append(",\"ts\":").Append(F(Time.timeScale))
            .Append(",\"dt\":").Append(F(Time.deltaTime))
            .Append(",\"ff\":").Append(FastForward() ? "true" : "false");

        DescribeNewBloons(sim);

        var every = Math.Max(1, (int)BloonsMLMod.TracePositionEvery);
        if (sim.roundTime.elapsed % every == 0) // an der Rundenzeit ausgerichtet, damit zwei Aufnahmen dieselben Punkte haben
        {
            // je Bloon [id, Pfad-Index, Distanz, x, y]
            sb.Append(",\"b\":[");
            var first = true;
            var paths = sim.map.pathManager.paths;
            for (var p = 0; p < paths.Count; p++)
            {
                var list = paths[p].bloonsList.list;
                for (var i = 0; i < list.Count; i++)
                {
                    var bloon = list[i];
                    if (!first) sb.Append(',');
                    first = false;
                    var pos = bloon.Position;
                    sb.Append('[').Append(bloon.Id.Raw).Append(',').Append(p).Append(',').Append(F(bloon.distanceTraveled))
                        .Append(',').Append(F(pos.X)).Append(',').Append(F(pos.Y)).Append(']');
                }
            }
            sb.Append(']');
        }
        sb.Append('}');
        Emit(sb);
    }

    private static bool TryStart(Simulation sim)
    {
        try
        {
            var game = InGameData.CurrentGame;
            var mapName = sim.model?.map?.mapName ?? game?.selectedMap ?? "unknown_map";
            var mode = game?.selectedMode ?? "unknownMode";
            var difficulty = game?.selectedDifficulty ?? "unknownDifficulty";
            var dir = RecordingsDir();
            var file = $"{DateTime.Now:yyyyMMdd-HHmmss}_{mapName}_{mode}_{difficulty}.trace.jsonl.gz";
            writer = new TraceWriter(Path.Combine(dir, OutputPaths.Sanitize(file)));
            lastUnityFrame = -1;
            describedBloons.Clear();
            lastFastForward = FastForward(); // Zustand steht im Header, kein Ereignis im ersten Schritt

            var paths = new JArray();
            var simPaths = sim.map.pathManager.paths;
            for (var p = 0; p < simPaths.Count; p++)
            {
                var path = simPaths[p];
                paths.Add(new JObject
                {
                    ["index"] = p,
                    ["pathId"] = path.def?.pathId,
                    ["spawnDist"] = path.spawnDist,
                    ["leakDist"] = path.leakDist,
                    ["maximumDistanceUntilSpawn"] = path.maximumDistanceUntilSpawn,
                    ["maximumDistanceUntilLeak"] = path.maximumDistanceUntilLeak,
                    ["segments"] = path.segments?.Length,
                    ["isActive"] = path.isActive
                });
            }
            var header = new JObject
            {
                ["ev"] = "header",
                ["format"] = FormatVersion,
                ["modVersion"] = typeof(BloonsMLMod).Assembly.GetName().Version?.ToString(),
                ["gameVersion"] = Application.version,
                ["map"] = mapName,
                ["mode"] = mode,
                ["difficulty"] = difficulty,
                ["positionEverySteps"] = Math.Max(1, (int)BloonsMLMod.TracePositionEvery),
                ["startTime"] = sim.time.elapsed,
                ["round"] = CurrentRound(),
                ["mapWideBloonSpeed"] = sim.map.mapWideBloonSpeed,
                ["fastForward"] = FastForward(),
                ["paths"] = paths
            };
            writer.Enqueue(header.ToString(Formatting.None));
            BloonsMLMod.Log.Msg($"Trace startet -> {writer.Path}");
            return true;
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Error($"Trace konnte nicht starten: {e}");
            var failed = writer;
            writer = null;
            if (failed != null)
            {
                failed.Dispose();
                try
                {
                    File.Delete(failed.Path); // leere/kaputte Datei nicht liegen lassen
                }
                catch (Exception)
                {
                    // egal
                }
            }
            BloonsMLMod.RecordTrace.SetValue(false); // nicht jeden Schritt erneut versuchen
            return false;
        }
    }

    /// <summary>Neben dem Dump-Ordner: &lt;DumpRoot&gt;/../recordings, mit eigener .gitignore (Ninja-Kiwi-Daten, öffentliches Repo).</summary>
    private static string RecordingsDir()
    {
        var dumpRoot = OutputPaths.ToHostPath(BloonsMLMod.DumpRoot).TrimEnd('\\', '/');
        var dir = Path.Combine(Path.GetDirectoryName(dumpRoot)!, "recordings");
        Directory.CreateDirectory(dir);
        var gitignore = Path.Combine(dir, ".gitignore");
        if (!File.Exists(gitignore)) File.WriteAllText(gitignore, "*\n");
        return dir;
    }

    // ------------------------------------------------------------------ Ereignisse aus Mod-Helper-Hooks

    public static void OnRoundStart() => RoundEvent("round_start");

    public static void OnRoundEnd() => RoundEvent("round_end");

    private static void RoundEvent(string name)
    {
        if (!Active) return;
        Emit(Event(name).Append(",\"round\":").Append(CurrentRound()).Append('}'));
    }

    public static void OnBloonCreated(Bloon bloon)
    {
        if (!Active) return;
        var sb = Event("spawn");
        AppendBloon(sb, bloon);
        sb.Append(",\"spawnRound\":").Append(bloon.spawnRound)
            .Append(",\"emissionIndex\":").Append(bloon.emissionIndex)
            .Append(",\"createdAtRoundTime\":").Append(bloon.createdAtRoundTime)
            .Append(",\"health\":").Append(bloon.health)
            .Append(",\"lineage\":[");
        var lineage = bloon.lineage;
        if (lineage != null)
        {
            for (var i = 0; i < lineage.Count; i++)
            {
                if (i > 0) sb.Append(',');
                sb.Append(lineage[i].Raw);
            }
        }
        sb.Append("]}");
        Emit(sb);
    }

    /// <summary>Jeden Schritt (unabhängig von der Abtastung): neue Bloons einmal beschreiben.</summary>
    private static void DescribeNewBloons(Simulation sim)
    {
        if (describedBloons.Count > 100_000) describedBloons.Clear(); // Freeplay: Speicher begrenzen, schlimmstenfalls doppelt
        var paths = sim.map.pathManager.paths;
        for (var p = 0; p < paths.Count; p++)
        {
            var list = paths[p].bloonsList.list;
            for (var i = 0; i < list.Count; i++)
            {
                var bloon = list[i];
                if (describedBloons.Add(bloon.Id.Raw)) DescribeBloon(bloon);
            }
        }
    }

    /// <summary>Erster Schritt eines Bloons auf dem Pfad: Werte, die beim Erzeugen noch fehlen (Tempo, Leben, Radius).</summary>
    private static void DescribeBloon(Bloon bloon)
    {
        var model = bloon.bloonModel;
        var sb = Event("bloon_info");
        AppendBloon(sb, bloon);
        sb.Append(",\"health\":").Append(bloon.health)
            .Append(",\"trackSpeedMultiplier\":").Append(F(bloon.trackSpeedMultiplier))
            .Append(",\"totalSpeedFrames\":").Append(F(bloon.TotalSpeedFrames))
            .Append(",\"modelSpeed\":").Append(F(model?.speed ?? double.NaN))
            .Append(",\"modelSpeedFrames\":").Append(F(model?.speedFrames ?? double.NaN))
            .Append(",\"radius\":").Append(F(model?.radius ?? double.NaN))
            .Append('}');
        Emit(sb);
    }

    public static void OnBloonLeaked(Bloon bloon, bool after)
    {
        if (!Active) return;
        if (after) describedBloons.Remove(bloon.Id.Raw);
        var sb = Event(after ? "leak_done" : "leak");
        AppendBloon(sb, bloon);
        sb.Append(",\"health\":").Append(bloon.health)
            .Append(",\"leakDamage\":").Append(F(bloon.bloonModel?.leakDamage ?? 0))
            .Append(",\"lives\":").Append(F(Lives()))
            .Append('}');
        Emit(sb);
    }

    public static void OnTower(string what, Tower tower, string detail = null)
    {
        if (!Active) return;
        var pos = tower.Position;
        var sb = Event(what);
        sb.Append(",\"id\":").Append(tower.Id.Raw)
            .Append(",\"tower\":").Append(JsonConvert.ToString(tower.towerModel?.name))
            .Append(",\"x\":").Append(F(pos.X)).Append(",\"y\":").Append(F(pos.Y))
            .Append(",\"range\":").Append(F(tower.towerModel?.range ?? 0));
        if (detail != null) sb.Append(",\"detail\":").Append(JsonConvert.ToString(detail));
        sb.Append('}');
        Emit(sb);
    }

    public static void OnWeaponFire(Weapon weapon)
    {
        if (!Active) return;
        var tower = weapon.attack?.tower;
        var sb = Event("shot");
        sb.Append(",\"tower\":").Append(tower != null ? tower.Id.Raw : -1)
            .Append(",\"weapon\":").Append(JsonConvert.ToString(weapon.weaponModel?.name))
            .Append(",\"rateFrames\":").Append(weapon.rateFrames)
            .Append('}');
        Emit(sb);
    }

    // ------------------------------------------------------------------ Hilfen

    /// <summary>Vorspulen: Mod Helpers OnFastForwardChanged wird in 3.6.10 nicht mehr aufgerufen.</summary>
    private static bool FastForward()
    {
        try
        {
            return TimeManager.FastForwardActive;
        }
        catch (Exception)
        {
            return false;
        }
    }

    private static Simulation Sim => InGame.instance?.bridge?.Simulation;

    private static StringBuilder Event(string name)
    {
        var sim = Sim;
        var sb = new StringBuilder(160);
        sb.Append("{\"ev\":\"").Append(name).Append('"');
        if (sim != null) sb.Append(",\"t\":").Append(sim.time.elapsed).Append(",\"rt\":").Append(sim.roundTime.elapsed);
        return sb;
    }

    private static void AppendBloon(StringBuilder sb, Bloon bloon)
    {
        var pos = bloon.Position;
        sb.Append(",\"id\":").Append(bloon.Id.Raw)
            .Append(",\"type\":").Append(JsonConvert.ToString(bloon.bloonModel?.id))
            .Append(",\"dist\":").Append(F(bloon.distanceTraveled))
            .Append(",\"x\":").Append(F(pos.X)).Append(",\"y\":").Append(F(pos.Y));
    }

    private static int CurrentRound()
    {
        try
        {
            return InGame.instance?.bridge?.GetCurrentRound() ?? -1;
        }
        catch (Exception)
        {
            return -1;
        }
    }

    private static double Lives()
    {
        try
        {
            return Sim?.health?.Value ?? double.NaN;
        }
        catch (Exception)
        {
            return double.NaN;
        }
    }

    private static string F(double v) => double.IsNaN(v) ? "null" : v.ToString("R", Inv);

    private static void Emit(StringBuilder sb) => writer?.Enqueue(sb.ToString());
}

/// <summary>Ein Eintrag pro Sim-Schritt. MelonLoader wendet Harmony-Patches der Mod automatisch an.</summary>
[HarmonyPatch(typeof(Simulation), nameof(Simulation.Simulate))]
internal static class Simulation_Simulate_Trace
{
    [HarmonyPostfix]
    internal static void Postfix(Simulation __instance)
    {
        try
        {
            TraceRecorder.OnSimulate(__instance);
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Error($"Trace-Schritt fehlgeschlagen, Trace wird beendet: {e}");
            TraceRecorder.Stop("Fehler");
            BloonsMLMod.RecordTrace.SetValue(false);
        }
    }
}
