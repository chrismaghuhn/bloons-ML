using BloonsML;
using BloonsML.Common;
using BloonsML.Dump;
using BTD_Mod_Helper;
using BTD_Mod_Helper.Api.ModOptions;
using Il2CppAssets.Scripts.Models;
using Il2CppAssets.Scripts.Models.Map;
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

    public static MelonLogger.Instance Log => Melon<BloonsMLMod>.Logger;

    public override void OnApplicationStart()
    {
        Log.Msg($"BloonsML geladen. Dump-Ziel: {OutputPaths.ToHostPath(DumpRoot)}  (F6 = alles, F7 = Karte)");
    }

    public override void OnUpdate()
    {
        if (DumpAllHotkey.JustPressed()) GameDataDumper.Start();
        if (DumpMapHotkey.JustPressed()) MapDumper.DumpLast(force: true);
    }

    public override void OnNewGameModel(GameModel result, MapModel map)
    {
        MapDumper.Remember(result, map);
        if (AutoDumpMaps) MapDumper.DumpLast(force: false);
    }
}
