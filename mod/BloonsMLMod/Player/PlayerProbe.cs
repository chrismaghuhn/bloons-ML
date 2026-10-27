using System;
using System.Collections;
using System.IO;
using System.Linq;
using BloonsML.Common;
using BTD_Mod_Helper.Extensions;
using HarmonyLib;
using Il2CppAssets.Scripts;
using Il2CppAssets.Scripts.Models.Towers;
using Il2CppAssets.Scripts.Simulation;
using Il2CppAssets.Scripts.Simulation.Towers;
using Il2CppAssets.Scripts.Unity.Bridge;
using Il2CppAssets.Scripts.Unity.UI_New.InGame;
using MelonLoader;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEngine;

namespace BloonsML.Player;

/// <summary>
/// Test-Mod für den Replay-Player (docs/design-replay-player.md, Schritt 1): prüft im laufenden Spiel die Punkte,
/// die das Design als [prüfen] markiert. Aufruf vor dem ersten Rundenstart einer Wegwerf-Partie (Monkey Meadow,
/// CHIMPS) per F10. Schreibt &lt;DumpRoot&gt;/../recordings/&lt;Zeit&gt;_probe.json und schaltet den Trace an.
/// Verändert die Partie (Türme, Geld, Leben, Runde) – nur offline und in einer Partie, die nicht zählt.
/// Läuft als Coroutine: nach jeder Aktion wird auf Callback und leere Bridge-Warteschlange gewartet, und der
/// Callback trägt ein, in welchem Frame/Sim-Schritt er wirklich kam.
/// </summary>
public static class PlayerProbe
{
    /// <summary>
    /// Rundenzeiten (Sim-Schritte), zu denen aus dem Simulate-Prefix je ein Turm gesetzt wird: drei aufeinanderfolgende,
    /// damit vorgespult (3 Schritte je Frame) jede Position im Frame einmal vorkommt.
    /// </summary>
    private static readonly int[] MidRoundSteps = { 300, 301, 302 };

    /// <summary>Freie Plätze für Teil 2 (Monkey Meadow, Land); genommen wird jeweils der erste, der gerade frei ist.</summary>
    private static readonly (float x, float y)[] MidRoundSpots = { (105f, -41f), (60f, 60f), (105f, -61f), (85f, -41f), (60f, 80f) };

    /// <summary>Ziel-Runde für SetRound (interne Zählung; CHIMPS beginnt intern bei 5 = Runde 6).</summary>
    private const int TargetRoundInternal = 9;

    /// <summary>So viele Frames höchstens auf Callback/leere Warteschlange warten.</summary>
    private const int MaxWaitFrames = 180;

    private static JObject report;
    private static string reportPath;
    private static bool running;

    /// <summary>Simulation, für die Teil 1 gelaufen ist; Teil 2 feuert nur dort (nie in einer späteren Partie).</summary>
    private static Simulation probedSim;
    private static readonly System.Collections.Generic.HashSet<int> pendingMidRound = new();
    private static bool midRoundArmed => pendingMidRound.Count > 0;

    // Position des aktuellen Sim-Schritts im Unity-Frame (1 = erster Schritt des Frames)
    private static int prefixFrame = -1;
    private static int prefixStepInFrame;

    public static void Run()
    {
        var bridge = InGame.instance?.bridge;
        if (bridge?.Simulation == null)
        {
            BloonsMLMod.Log.Warning("Player-Test: erst eine Partie starten (vor dem ersten Rundenstart).");
            return;
        }
        if (running || (probedSim != null && probedSim.Pointer == bridge.Simulation.Pointer))
        {
            BloonsMLMod.Log.Warning("Player-Test läuft bereits bzw. ist in dieser Partie schon gelaufen (neue Partie starten).");
            return;
        }
        if (bridge.AreRoundsActive())
        {
            BloonsMLMod.Log.Warning("Player-Test: nur vor dem ersten Rundenstart (zwischen den Runden) möglich.");
            return;
        }
        running = true;
        cancelled = false;
        MelonCoroutines.Start(Guarded(RunPart1(bridge)));
    }

    /// <summary>Partie beendet/neu gestartet/Hauptmenü: Teil 2 entschärfen.</summary>
    public static void Reset()
    {
        pendingMidRound.Clear();
        probedSim = null;
        cancelled = true;
        running = false; // falls eine verschachtelte Coroutine abgebrochen ist und Guarded nie zu Ende lief
    }

    private static bool cancelled;

    /// <summary>Läuft der Test noch in der Partie, in der er gestartet wurde?</summary>
    private static bool Alive()
    {
        var sim = InGame.instance?.bridge?.Simulation;
        return !cancelled && probedSim != null && sim != null && sim.Pointer == probedSim.Pointer;
    }

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
                BloonsMLMod.Log.Error($"Player-Test abgebrochen: {e}");
                break;
            }
            yield return current;
        }
        running = false;
        Save();
    }

    private static IEnumerator RunPart1(UnityToSimulation bridge)
    {
        probedSim = bridge.Simulation;
        report = new JObject
        {
            ["format"] = "bloonsml-player-probe/1",
            ["gameVersion"] = Application.version,
            ["map"] = bridge.Simulation.model?.map?.mapName,
            ["mode"] = InGameData.CurrentGame?.selectedMode,
            ["difficulty"] = InGameData.CurrentGame?.selectedDifficulty,
            ["checks"] = new JArray()
        };
        reportPath = Path.Combine(OutputPaths.RecordingsDir(), $"{DateTime.Now:yyyyMMdd-HHmmss-fff}_probe.json");
        BloonsMLMod.Log.Msg($"Player-Test startet -> {reportPath}");

        // Trace an und warten, bis er wirklich schreibt (startet mit dem nächsten Sim-Schritt)
        if (!BloonsMLMod.RecordTrace)
        {
            BloonsMLMod.RecordTrace.SetValue(true); // nur für diese Sitzung, nicht gespeichert
            BloonsMLMod.Log.Msg("Player-Test: Trace für diese Sitzung eingeschaltet (F9 schaltet ihn aus)");
        }
        for (var i = 0; i < MaxWaitFrames && !Recorder.TraceRecorder.Active; i++) yield return null;
        if (!Alive()) yield break;

        var dart = FindTower(bridge, "DartMonkey");
        Check("state_before", State(bridge));
        bridge.SetCash(5000, bridge.GetInputId());

        // --- A: Platzieren an exakter und krummer Koordinate (Plätze aus Sim-Replays auf Monkey Meadow)
        var a1 = new PlaceResult();
        yield return Place(bridge, dart, -35.0f, 39.0f, "place_exact", a1);
        if (!Alive()) yield break;
        var a2 = new PlaceResult();
        yield return Place(bridge, dart, -35.37f, 59.81f, "place_fractional", a2);
        if (!Alive()) yield break;

        // --- B: Fehlerfälle (belegt nur, wenn der Platz wirklich belegt ist)
        if (a1.Tower != null) yield return Place(bridge, dart, -35.0f, 39.0f, "place_occupied", new PlaceResult());
        else Check("place_occupied", new JObject { ["skipped"] = "place_exact hat keinen Turm erzeugt" });
        if (!Alive()) yield break;
        yield return Place(bridge, dart, -100.0f, -18.9f, "place_on_track", new PlaceResult());
        if (!Alive()) yield break;
        bridge.SetCash(10, bridge.GetInputId());
        yield return Place(bridge, dart, 60.0f, 60.0f, "place_no_cash", new PlaceResult());
        if (!Alive()) yield break;
        bridge.SetCash(5000, bridge.GetInputId());

        // --- C: Upgrade (mit Geld, dann ohne)
        if (a1.Tower != null) yield return Upgrade(bridge, a1.Tower, 0, "upgrade_with_cash");
        if (!Alive()) yield break;
        if (a2.Tower != null)
        {
            bridge.SetCash(1, bridge.GetInputId());
            yield return Upgrade(bridge, a2.Tower, 1, "upgrade_no_cash");
        }
        if (!Alive()) yield break;

        // --- D: Geld und Leben setzen (Bruchteil, CHIMPS max. 1 Leben)
        bridge.SetCash(1234.5, bridge.GetInputId());
        Check("set_cash_fraction", new JObject { ["set"] = 1234.5, ["read"] = bridge.GetCash(bridge.GetInputId()) });
        var sim = bridge.Simulation;
        var healthBefore = sim.health.Value;
        sim.health.Value = 5;
        Check("set_health_5", new JObject
        {
            ["before"] = healthBefore, ["set"] = 5, ["read"] = sim.health.Value, ["max"] = sim.maxHealth.Value
        });
        yield return null;
        if (!Alive()) yield break;
        Check("health_next_frame", new JObject { ["read"] = sim.health.Value, ["max"] = sim.maxHealth.Value });

        // --- E: bridge.SetRound vor dem ersten Rundenstart (welche Runde startet danach? -> Trace round_start/spawn)
        var roundBefore = bridge.GetCurrentRound();
        bridge.SetRound(TargetRoundInternal, false);
        Check("set_round", new JObject
        {
            ["method"] = "bridge.SetRound(round, false)",
            ["before"] = roundBefore, ["set"] = TargetRoundInternal, ["after"] = bridge.GetCurrentRound(),
            ["spawnedRound"] = sim.map.spawner.spawnedRound
        });

        Check("state_after", State(bridge));
        foreach (var step in MidRoundSteps) pendingMidRound.Add(step);
        Save();
        BloonsMLMod.Log.Msg($"Player-Test Teil 1 fertig. Jetzt die Runde starten und vorspulen; bei den Rundenschritten " +
                            $"{string.Join(", ", MidRoundSteps)} setzt der Test je einen Turm (Teil 2) und ergänzt die Datei.");
    }

    /// <summary>Teil F: aus dem Simulate-Prefix heraus platzieren, wenn die Rundenzeit erreicht ist.</summary>
    internal static void OnSimulatePrefix(Simulation sim)
    {
        if (Time.frameCount != prefixFrame)
        {
            prefixFrame = Time.frameCount;
            prefixStepInFrame = 0;
        }
        prefixStepInFrame++;
        if (!midRoundArmed || probedSim == null || probedSim.Pointer != sim.Pointer) return;
        var roundTime = sim.roundTime.elapsed;
        if (!pendingMidRound.Remove(roundTime)) return;
        var bridge = InGame.instance?.bridge;
        if (bridge == null) return;
        var dart = FindTower(bridge, "DartMonkey");
        var spot = MidRoundSpots.FirstOrDefault(p =>
            bridge.CanPlaceTowerAt(new Vector2(p.x, p.y), dart, bridge.GetInputId(), ObjectId.Invalid));
        var entry = new JObject
        {
            ["roundTimeAtCall"] = roundTime,
            ["simTimeAtCall"] = sim.time.elapsed,
            ["frameAtCall"] = Time.frameCount,
            ["stepInFrameAtCall"] = prefixStepInFrame,
            ["fastForward"] = TimeManager.FastForwardActive
        };
        Check($"mid_round_prefix_place_{roundTime}", entry);
        if (spot == default)
        {
            entry["skipped"] = "kein freier Platz";
            return;
        }
        // Aufruf und Sofort-Ergebnis laufen hier im Prefix; Warten auf Callback/Turm danach per Coroutine
        MelonCoroutines.Start(Guarded(PlaceInto(bridge, dart, spot.x, spot.y, entry, new PlaceResult())));
    }

    // ------------------------------------------------------------------ Prüfungen

    private sealed class PlaceResult
    {
        public Tower Tower;
    }

    private static IEnumerator Place(UnityToSimulation bridge, TowerModel model, float x, float y, string name,
        PlaceResult result)
    {
        var entry = new JObject();
        Check(name, entry);
        yield return PlaceInto(bridge, model, x, y, entry, result);
    }

    /// <summary>
    /// Ruft CreateTowerAt auf und trägt in <paramref name="entry"/> ein: Abfrage vorher, Sofort-Ergebnis direkt nach
    /// dem Aufruf, und – auch wenn er später kommt – Callback mit Frame und Sim-Zeit; dann neuer Turm und Position.
    /// </summary>
    private static IEnumerator PlaceInto(UnityToSimulation bridge, TowerModel model, float x, float y, JObject entry,
        PlaceResult result)
    {
        var sim = bridge.Simulation;
        var inputId = bridge.GetInputId();
        var pos = new Vector2(x, y);
        var query = bridge.QueryPlacementPosition(pos, model, inputId, ObjectId.Invalid);
        entry["request"] = new JArray(x, y);
        entry["query"] = new JObject
        {
            ["positionValid"] = query?.positionValid,
            ["position"] = query == null ? null : new JArray(query.position.x, query.position.y)
        };
        entry["canPlace"] = bridge.CanPlaceTowerAt(pos, model, inputId, ObjectId.Invalid);
        var before = TowerIds();
        entry["cashBefore"] = bridge.GetCash(inputId);
        entry["callFrame"] = Time.frameCount;
        entry["callSimTime"] = sim.time.elapsed;
        var fired = false;
        var waitedOut = false;
        bridge.CreateTowerAt(inputId, pos, model, ObjectId.Invalid, false,
            (Il2CppSystem.Action<bool>)(Action<bool>)(ok =>
            {
                fired = true;
                entry["callback"] = ok;
                entry["callbackFrame"] = Time.frameCount;
                entry["callbackSimTime"] = bridge.Simulation?.time.elapsed;
                entry["callbackRoundTime"] = bridge.Simulation?.roundTime.elapsed;
                if (waitedOut) Save(); // kam nach Ablauf der Wartezeit: sonst ginge er verloren
            }),
            false, false, false, -1, true, -1);
        entry["callbackFiredDuringCall"] = fired;
        entry["queuedActionsAfterCall"] = bridge.actions.Count;
        entry["towerCreatedDuringCall"] = TowerIds().Except(before).Any();

        // warten, bis Callback da und Warteschlange leer (oder Zeit um)
        var waited = 0;
        while ((!fired || bridge.actions.Count > 0) && waited < MaxWaitFrames && Alive())
        {
            waited++;
            yield return null;
        }
        entry["waitedFrames"] = waited;
        entry["callbackFired"] = fired;
        waitedOut = true;
        if (!Alive())
        {
            entry["aborted"] = "Partie verlassen";
            Save();
            yield break;
        }

        // neuer Turm an der angefragten Stelle (mehrere Platzierungen können gleichzeitig laufen)
        // neuer Turm an der angefragten Stelle (mehrere Platzierungen können gleichzeitig laufen); rastet das Spiel
        // doch einmal ein, der nächstgelegene neue Turm (positionDelta zeigt es dann)
        var fresh = InGame.instance.GetTowers().Where(t => !before.Contains(t.Id.Raw)).ToList();
        result.Tower = fresh.FirstOrDefault(t => Math.Abs(t.Position.X - x) < 1e-3 && Math.Abs(t.Position.Y - y) < 1e-3)
                       ?? fresh.OrderBy(t => (t.Position.X - x) * (t.Position.X - x) + (t.Position.Y - y) * (t.Position.Y - y))
                           .FirstOrDefault();
        entry["created"] = result.Tower != null;
        entry["cashAfter"] = bridge.GetCash(inputId);
        if (result.Tower != null)
        {
            var p = result.Tower.Position;
            entry["createdPosition"] = new JArray(p.X, p.Y);
            entry["positionDelta"] = Math.Sqrt((p.X - x) * (p.X - x) + (p.Y - y) * (p.Y - y));
            entry["worth"] = result.Tower.worth;
        }
        Save();
    }

    private static IEnumerator Upgrade(UnityToSimulation bridge, Tower tower, int path, string name)
    {
        var inputId = bridge.GetInputId();
        var sim = bridge.Simulation;
        var entry = new JObject
        {
            ["path"] = path,
            ["tiersBefore"] = string.Join("", tower.towerModel.tiers),
            ["cashBefore"] = bridge.GetCash(inputId),
            ["callFrame"] = Time.frameCount,
            ["callSimTime"] = sim.time.elapsed
        };
        Check(name, entry);
        var fired = false;
        var waitedOut = false;
        bridge.UpgradeTower(inputId, tower.Id, path, 0,
            (Il2CppSystem.Action<bool>)(Action<bool>)(ok =>
            {
                fired = true;
                entry["callback"] = ok;
                entry["callbackFrame"] = Time.frameCount;
                entry["callbackSimTime"] = bridge.Simulation?.time.elapsed;
                if (waitedOut) Save();
            }));
        entry["callbackFiredDuringCall"] = fired;
        entry["queuedActionsAfterCall"] = bridge.actions.Count;
        var waited = 0;
        while ((!fired || bridge.actions.Count > 0) && waited < MaxWaitFrames && Alive())
        {
            waited++;
            yield return null;
        }
        entry["waitedFrames"] = waited;
        entry["callbackFired"] = fired;
        waitedOut = true;
        if (!Alive())
        {
            entry["aborted"] = "Partie verlassen";
            Save();
            yield break;
        }
        var current = bridge.GetTowerFromId(tower.Id);
        entry["tiersAfter"] = current == null ? null : string.Join("", current.towerModel.tiers);
        entry["cashAfter"] = bridge.GetCash(inputId);
        entry["worthAfter"] = current?.worth;
        Save();
    }

    private static JObject State(UnityToSimulation bridge)
    {
        var sim = bridge.Simulation;
        return new JObject
        {
            ["cash"] = bridge.GetCash(bridge.GetInputId()),
            ["health"] = sim.health.Value,
            ["maxHealth"] = sim.maxHealth.Value,
            ["currentRound"] = bridge.GetCurrentRound(),
            ["spawnedRound"] = sim.map.spawner.spawnedRound,
            ["roundsActive"] = bridge.AreRoundsActive(),
            ["simTime"] = sim.time.elapsed,
            ["towers"] = new JArray(InGame.instance.GetTowers().Select(t => new JObject
            {
                ["id"] = t.Id.Raw,
                ["name"] = t.towerModel?.name,
                ["x"] = t.Position.X,
                ["y"] = t.Position.Y,
                ["worth"] = t.worth
            }))
        };
    }

    // ------------------------------------------------------------------ Hilfen

    private static TowerModel FindTower(UnityToSimulation bridge, string baseId) =>
        bridge.Model.towers.FirstOrDefault(t => t.name == baseId);

    private static System.Collections.Generic.HashSet<uint> TowerIds() =>
        InGame.instance.GetTowers().Select(t => t.Id.Raw).ToHashSet();

    private static void Check(string name, JObject entry)
    {
        entry["check"] = name;
        ((JArray)report["checks"]).Add(entry);
        BloonsMLMod.Log.Msg($"Player-Test {name}");
    }

    private static void Save()
    {
        if (report == null || reportPath == null) return;
        try
        {
            File.WriteAllText(reportPath, report.ToString(Formatting.Indented));
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Error($"Player-Test: {reportPath}: {e.Message}");
        }
    }
}

/// <summary>Teil F des Player-Tests: Aktion aus dem Prefix eines Sim-Schritts.</summary>
[HarmonyPatch(typeof(Simulation), nameof(Simulation.Simulate))]
internal static class Simulation_Simulate_PlayerProbe
{
    [HarmonyPrefix]
    internal static void Prefix(Simulation __instance)
    {
        try
        {
            PlayerProbe.OnSimulatePrefix(__instance);
        }
        catch (Exception e)
        {
            BloonsMLMod.Log.Error($"Player-Test Teil 2: {e}");
        }
    }
}
