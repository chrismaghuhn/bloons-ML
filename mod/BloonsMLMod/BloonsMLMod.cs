using BloonsML;
using BloonsML.Common;
using BloonsML.Dump;
using BloonsML.Recorder;
using BTD_Mod_Helper;
using System.Collections.Generic;
using BTD_Mod_Helper.Api.ModOptions;
using Il2CppAssets.Scripts.Models;
using Il2CppAssets.Scripts.Models.Gameplay.Mods;
using Il2CppAssets.Scripts.Models.Map;
using Il2CppAssets.Scripts.Models.Towers;
using Il2CppAssets.Scripts.Simulation.Bloons;
using Il2CppAssets.Scripts.Simulation.Towers;
using Il2CppAssets.Scripts.Simulation.Towers.Weapons;
using MelonLoader;
using UnityEngine;

[assembly: MelonInfo(typeof(BloonsMLMod), "BloonsML", "0.1.0", "chrismaghuhn")]
[assembly: MelonGame("Ninja Kiwi", "BloonsTD6")]
[assembly: MelonGame("Ninja Kiwi", "BloonsTD6-Epic")]

namespace BloonsML;

/// <summary>
/// Einstiegspunkt der bloons-ML-Mod.
/// Stufe 1: Dump der Spiel-Models (Türme, Upgrades, Bloons, Runden, Karten) als JSON.
/// Später: Recorder (echte Partien aufzeichnen) und Live-Bridge (Agent spielt per Socket).
/// </summary>
public class BloonsMLMod : BloonsTD6Mod
{
    public static readonly ModSettingString DumpRoot = new(OutputPaths.DefaultDumpRoot)
    {
        displayName = "Dump-Ordner",
        description = "Zielordner für den Daten-Dump (Unix-Pfad wie /home/... wird automatisch nach Z:\\ umgerechnet). " +
                      "Darunter entsteht <Spielversion>/."
    };

    public static readonly ModSettingHotkey DumpAllHotkey = new(KeyCode.F6)
    {
        displayName = "Hotkey: kompletter Dump",
        description = "Schreibt Türme (alle Upgrade-Kombinationen), Upgrades, Bloons, Runden-Sets, Karten-Index, GameData und GameModel."
    };

    public static readonly ModSettingHotkey DumpMapHotkey = new(KeyCode.F7)
    {
        displayName = "Hotkey: aktuelle Karte dumpen",
        description = "Schreibt die zuletzt geladene Karte (Pfade, Platzierungsflächen) und ihre Partie-Regeln erneut."
    };

    public static readonly ModSettingBool AutoDumpMaps = new(true)
    {
        displayName = "Karten automatisch dumpen",
        description = "Beim Start jeder Partie: Karte nach maps/<Karte>.json (nur wenn noch nicht vorhanden) und " +
                      "Partie-Regeln nach matches/<Modus>_<Schwierigkeit>_<Karte>.json."
    };

    public static readonly ModSettingButton DumpAllButton = new()
    {
        displayName = "Kompletten Dump starten",
        buttonText = "Dump",
        action = GameDataDumper.Start
    };

    public static readonly ModSettingBool RecordTrace = new(false)
    {
        displayName = "Trace aufzeichnen (Test-Recorder)",
        description = "Schreibt pro Sim-Schritt Zeitzähler und Bloon-Positionen sowie Spawn/Leak/Runde/Turm/Schuss nach " +
                      "<DumpRoot>/../recordings/*.trace.jsonl.gz. Für Kalibrier-Partien; kostet etwas Leistung.",
        onValueChanged = TraceRecorder.SetEnabled
    };

    public static readonly ModSettingInt TracePositionEvery = new(1)
    {
        displayName = "Trace: Positionen alle N Sim-Schritte",
        description = "1 = jeder Schritt (Kalibrierung), 6 = 0,1 s bei 60 Schritten/s.",
        min = 1,
        max = 60
    };

    public static readonly ModSettingHotkey ToggleTraceHotkey = new(KeyCode.F9)
    {
        displayName = "Hotkey: Trace an/aus",
        description = "Schaltet 'Trace aufzeichnen' um."
    };

    public static MelonLogger.Instance Log => Melon<BloonsMLMod>.Logger;

    public override void OnApplicationStart()
    {
        Log.Msg($"BloonsML geladen. Dump-Ziel: {OutputPaths.ToHostPath(DumpRoot)}  (F6 = alles, F7 = Karte)");
    }

    public override void OnUpdate()
    {
        if (DumpAllHotkey.JustPressed()) GameDataDumper.Start();
        if (DumpMapHotkey.JustPressed()) MapDumper.DumpLast(force: true, includeMatch: true);
        if (ToggleTraceHotkey.JustPressed()) RecordTrace.SetValueAndSave(!RecordTrace);
    }

    // --- Recorder-Hooks (Test-Recorder, docs/design-recorder.md Schritt 1)

    public override void OnRoundStart() => TraceRecorder.OnRoundStart();

    public override void OnRoundEnd() => TraceRecorder.OnRoundEnd();

    public override void OnFastForwardChanged(bool newValue) => TraceRecorder.OnFastForwardChanged(newValue);

    public override void OnBloonCreated(Bloon bloon) => TraceRecorder.OnBloonCreated(bloon);

    public override bool PreBloonLeaked(Bloon bloon)
    {
        TraceRecorder.OnBloonLeaked(bloon, after: false);
        return true;
    }

    public override void PostBloonLeaked(Bloon bloon) => TraceRecorder.OnBloonLeaked(bloon, after: true);

    public override void OnTowerPlaced(Tower tower) => TraceRecorder.OnTower("tower_placed", tower);

    public override void OnTowerUpgraded(Tower tower, string upgradeName, TowerModel newBaseTowerModel) =>
        TraceRecorder.OnTower("tower_upgraded", tower, upgradeName);

    public override void OnTowerSold(Tower tower, float amount) =>
        TraceRecorder.OnTower("tower_sold", tower, amount.ToString(System.Globalization.CultureInfo.InvariantCulture));

    public override void OnWeaponFire(Weapon weapon) => TraceRecorder.OnWeaponFire(weapon);

    public override void OnMatchEnd() => TraceRecorder.Stop("Partie beendet");

    public override void OnRestart() => TraceRecorder.Stop("Neustart");

    public override void OnMainMenu() => TraceRecorder.Stop("Hauptmenü");

    public override void OnNewGameModel(GameModel result, IReadOnlyList<ModModel> mods) => MapDumper.RememberMods(result, mods);

    public override void OnNewGameModel(GameModel result, MapModel map)
    {
        MapDumper.Remember(result, map);
        if (AutoDumpMaps) MapDumper.DumpLast(force: false, includeMatch: false);
    }

    public override void LateOnNewGameModel(GameModel result)
    {
        MapDumper.RememberGame(result);
        if (AutoDumpMaps) MapDumper.DumpMatch();
    }
}
