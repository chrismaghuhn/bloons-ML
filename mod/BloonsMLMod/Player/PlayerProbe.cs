using System;
using System.Globalization;
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
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using UnityEngine;

namespace BloonsML.Player;

/// <summary>
/// Test-Mod für den Replay-Player (docs/design-replay-player.md, Schritt 1): prüft im laufenden Spiel die Punkte,
/// die das Design als [prüfen] markiert. Aufruf vor dem ersten Rundenstart einer Wegwerf-Partie (Monkey Meadow,
/// CHIMPS) per F10. Schreibt &lt;DumpRoot&gt;/../recordings/&lt;Zeit&gt;_probe.json und schaltet den Trace an.
/// Verändert die Partie (Türme, Geld, Leben, Runde) – nur offline und in einer Partie, die nicht zählt.
/// </summary>
public static class PlayerProbe
{
    /// <summary>Rundenzeit (Sim-Schritte), zu der aus dem Simulate-Prefix ein Turm gesetzt wird.</summary>
    private const int MidRoundStep = 300;

    /// <summary>Ziel-Runde für SetRound (interne Zählung; CHIMPS beginnt intern bei 5 = Runde 6).</summary>
    private const int TargetRoundInternal = 9;

    private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

    private static JObject report;
    private static string reportPath;
    private static bool midRoundArmed;

    public static void Run()
    {
        var inGame = InGame.instance;
        var bridge = inGame?.bridge;
        if (bridge?.Simulation == null)
        {
            BloonsMLMod.Log.Warning("Player-Test: erst eine Partie starten (vor dem ersten Rundenstart).");
            return;
        }
        if (!BloonsMLMod.RecordTrace) BloonsMLMod.RecordTrace.SetValueAndSave(true);

        report = new JObject
        {
            ["format"] = "bloonsml-player-probe/0",
            ["gameVersion"] = Application.version,
            ["map"] = bridge.Simulation.model?.map?.mapName,
            ["mode"] = InGameData.CurrentGame?.selectedMode,
            ["difficulty"] = InGameData.CurrentGame?.selectedDifficulty,
            ["checks"] = new JArray()
        };
        reportPath = Path.Combine(OutputPaths.RecordingsDir(), $"{DateTime.Now:yyyyMMdd-HHmmss}_probe.json");
        BloonsMLMod.Log.Msg("Player-Test startet");

        var dart = FindTower(bridge, "DartMonkey");
        Step("state_before", () => State(bridge));

        // --- A: Platzieren an exakter und krummer Koordinate (Plätze aus Sim-Replays auf Monkey Meadow)
        bridge.SetCash(5000, bridge.GetInputId());
        var a1 = Place(bridge, dart, -35.0f, 39.0f, "place_exact");
        var a2 = Place(bridge, dart, -35.37f, 59.81f, "place_fractional");

        // --- B: Fehlerfälle
        Place(bridge, dart, -35.0f, 39.0f, "place_occupied");
        Place(bridge, dart, -100.0f, -18.9f, "place_on_track");
        bridge.SetCash(10, bridge.GetInputId());
        Place(bridge, dart, 60.0f, 60.0f, "place_no_cash");
        bridge.SetCash(5000, bridge.GetInputId());

        // --- C: Upgrade (mit Geld, dann ohne)
        if (a1 != null) Upgrade(bridge, a1, 0, "upgrade_with_cash");
        if (a2 != null)
        {
            bridge.SetCash(1, bridge.GetInputId());
            Upgrade(bridge, a2, 1, "upgrade_no_cash");
        }

        // --- D: Geld und Leben setzen (Bruchteil, CHIMPS max. 1 Leben)
        Step("set_cash_fraction", () =>
        {
            bridge.SetCash(1234.5, bridge.GetInputId());
            return new JObject { ["set"] = 1234.5, ["read"] = bridge.GetCash(bridge.GetInputId()) };
        });
        Step("set_health_5", () =>
        {
            var sim = bridge.Simulation;
            var before = sim.health.Value;
            sim.health.Value = 5;
            return new JObject
            {
                ["before"] = before, ["set"] = 5, ["read"] = sim.health.Value, ["max"] = sim.maxHealth.Value
            };
        });

        // --- E: SetRound vor dem ersten Rundenstart (welche Runde startet danach? -> Trace round_start/spawns)
        Step("set_round", () =>
        {
            var before = bridge.GetCurrentRound();
            bridge.SetRound(TargetRoundInternal, false);
            return new JObject
            {
                ["before"] = before, ["set"] = TargetRoundInternal, ["after"] = bridge.GetCurrentRound(),
                ["spawnedRound"] = bridge.Simulation.map.spawner.spawnedRound
            };
        });

        Step("state_after", () => State(bridge));
        midRoundArmed = true;
        Save();
        BloonsMLMod.Log.Msg($"Player-Test Teil 1 fertig -> {reportPath}. Jetzt die Runde starten; bei Rundenschritt " +
                            $"{MidRoundStep} setzt der Test einen weiteren Turm (Teil 2).");
    }

    /// <summary>Teil F: aus dem Simulate-Prefix heraus platzieren, wenn die Rundenzeit erreicht ist.</summary>
    internal static void OnSimulatePrefix(Simulation sim)
    {
        if (!midRoundArmed || sim.roundTime.elapsed != MidRoundStep) return;
        midRoundArmed = false;
        var bridge = InGame.instance?.bridge;
        if (bridge == null) return;
        var dart = FindTower(bridge, "DartMonkey");
        Step("mid_round_prefix_place", () =>
        {
            var result = PlaceRaw(bridge, dart, 105.0f, -41.0f);
            result["roundTimeAtCall"] = MidRoundStep;
            return result;
        });
        Save();
        BloonsMLMod.Log.Msg($"Player-Test Teil 2 fertig -> {reportPath}");
    }

    // ------------------------------------------------------------------ einzelne Prüfungen

    private static Tower Place(UnityToSimulation bridge, TowerModel model, float x, float y, string name)
    {
        Tower created = null;
        Step(name, () =>
        {
            var result = PlaceRaw(bridge, model, x, y);
            created = lastCreated;
            return result;
        });
        return created;
    }

    private static Tower lastCreated;

    private static JObject PlaceRaw(UnityToSimulation bridge, TowerModel model, float x, float y)
    {
        var sim = bridge.Simulation;
        var inputId = bridge.GetInputId();
        var pos = new Vector2(x, y);
        var query = bridge.QueryPlacementPosition(pos, model, inputId, ObjectId.Invalid);
        var canPlace = bridge.CanPlaceTowerAt(pos, model, inputId, ObjectId.Invalid);
        var before = TowerIds(bridge);
        var cashBefore = bridge.GetCash(inputId);
        var tBefore = sim.time.elapsed;
        bool? callback = null;
        var callbackFrame = -1;
        bridge.CreateTowerAt(inputId, pos, model, ObjectId.Invalid, false,
            (Il2CppSystem.Action<bool>)(Action<bool>)(ok =>
            {
                callback = ok;
                callbackFrame = Time.frameCount;
            }),
            false, false, false, -1, true, -1);
        var queued = bridge.actions.Count;
        var after = InGame.instance.GetTowers();
        lastCreated = after.FirstOrDefault(t => !before.Contains(t.Id.Raw));
        var result = new JObject
        {
            ["request"] = new JArray(x, y),
            ["query"] = new JObject
            {
                ["positionValid"] = query?.positionValid,
                ["position"] = query == null ? null : new JArray(query.position.x, query.position.y)
            },
            ["canPlace"] = canPlace,
            ["callback"] = callback,
            ["callbackSameFrame"] = callback != null && callbackFrame == Time.frameCount,
            ["queuedActionsAfterCall"] = queued,
            ["created"] = lastCreated != null,
            ["cashBefore"] = cashBefore,
            ["cashAfter"] = bridge.GetCash(inputId),
            ["simTimeBefore"] = tBefore,
            ["simTimeAfter"] = sim.time.elapsed
        };
        if (lastCreated != null)
        {
            var p = lastCreated.Position;
            result["createdPosition"] = new JArray(p.X, p.Y);
            result["positionDelta"] = Math.Sqrt((p.X - x) * (p.X - x) + (p.Y - y) * (p.Y - y));
            result["worth"] = lastCreated.worth;
        }
        return result;
    }

    private static void Upgrade(UnityToSimulation bridge, Tower tower, int path, string name)
    {
        Step(name, () =>
        {
            var inputId = bridge.GetInputId();
            var tiersBefore = string.Join("", tower.towerModel.tiers);
            var cashBefore = bridge.GetCash(inputId);
            bool? callback = null;
            bridge.UpgradeTower(inputId, tower.Id, path, 0,
                (Il2CppSystem.Action<bool>)(Action<bool>)(ok => callback = ok));
            var current = bridge.GetTowerFromId(tower.Id);
            return new JObject
            {
                ["path"] = path,
                ["callback"] = callback,
                ["queuedActionsAfterCall"] = bridge.actions.Count,
                ["tiersBefore"] = tiersBefore,
                ["tiersAfter"] = current == null ? null : string.Join("", current.towerModel.tiers),
                ["cashBefore"] = cashBefore,
                ["cashAfter"] = bridge.GetCash(inputId),
                ["worthAfter"] = current?.worth
            };
        });
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

    private static System.Collections.Generic.HashSet<uint> TowerIds(UnityToSimulation bridge) =>
        InGame.instance.GetTowers().Select(t => t.Id.Raw).ToHashSet();

    private static void Step(string name, Func<JObject> check)
    {
        JObject entry;
        try
        {
            entry = check() ?? new JObject();
        }
        catch (Exception e)
        {
            entry = new JObject { ["exception"] = $"{e.GetType().Name}: {e.Message}" };
        }
        entry["check"] = name;
        ((JArray)report["checks"]).Add(entry);
        BloonsMLMod.Log.Msg($"Player-Test {name}: {entry.ToString(Formatting.None)}");
    }

    private static void Save()
    {
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
