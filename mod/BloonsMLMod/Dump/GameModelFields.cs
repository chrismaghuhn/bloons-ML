using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using Il2CppAssets.Scripts.Models;

namespace BloonsML.Dump;

/// <summary>Einteilung der GameModel-Felder für den Dump.</summary>
public static class GameModelFields
{
    /// <summary>Eigene Ordner (towers/, upgrades/, bloons/), Nachschlage-Duplikate davon und Laufzeit-Caches.</summary>
    public static readonly HashSet<string> Skip = new(StringComparer.OrdinalIgnoreCase)
    {
        "towers", "upgrades", "bloons", "upgradesByName", "bloonsByName", "powerProUpgradesByName", "_categorisedKnowledge",
        "searchCache", "cachedModels", "childDependants", "allowGarbageCollection"
    };

    /// <summary>
    /// Felder, die erst eine Partie setzt (Modus, Schwierigkeit, Startgeld/-leben, Runden-/Income-Set, Karte …).
    /// Im Menü-GameModel stehen hier nur Standardwerte; MapDumper schreibt sie aus dem GameModel der Partie
    /// nach matches/.
    /// </summary>
    public static readonly HashSet<string> Match = new(StringComparer.OrdinalIgnoreCase)
    {
        "map", "roundSet", "incomeSet", "difficultyId", "gameMode", "gameType", "subGameType", "reverseMode",
        "AllMods", "cash", "startingHealth", "maxHealth", "startingShield", "maxShield", "startingRevives",
        "maxSoftcapHealth", "softcapHealthPercentModifier", "startRound", "endRound", "randomSeed",
        "sellMultiplier", "sellValueMultiplier", "towerSellEnabled", "doubleCashAllowed", "continueEnabled",
        "goldenBloonActive", "isApopalypse", "isLoadedFromSave", "leastCashUsed", "leastTiersUsed", "ignoreLeastMode",
        "maxTowerCount", "maxPowerCount", "towerTierRestrictions", "spawnHeroesAtLevel", "heroXpMultiplier",
        "equalHeroXpDistribution", "stopEndlessWavesOnLastRound", "wasFastTrackApplied", "selectedDifficultyRating",
        "monkeyMoneyReward", "raceSendRoundDelay", "penaltyGraceTimeFrames", "penaltyGraceTimeSecondsPerRound",
        "bossBloonType", "bossEliteMode", "bossForcePreSpawnBoss", "bossRankedMode", "bossSpawnDistanceOverride",
        "bossSpawnRounds", "customMapDataModel", "mapEditorBehaviorModel",
        // im Menü null bzw. 0, werden erst mit der Partie gesetzt
        "behaviors", "globalBloonScale", "globalBossBloonScale", "globalTowerScale"
    };

    /// <summary>Alle lesbaren Felder von GameModel (ohne Compiler-Backing-Fields).</summary>
    public static IEnumerable<PropertyInfo> All() =>
        typeof(GameModel).GetProperties(BindingFlags.Public | BindingFlags.Instance)
            .Where(p => p.DeclaringType == typeof(GameModel) && p.CanRead && p.GetIndexParameters().Length == 0)
            .Where(p => !p.Name.Contains("k__BackingField"));

    /// <summary>Statische Spieldaten: gehören in gamemodel/.</summary>
    public static IEnumerable<PropertyInfo> Static() => All().Where(p => !Skip.Contains(p.Name) && !Match.Contains(p.Name));

    /// <summary>Partie-spezifische Felder: gehören in matches/.</summary>
    public static IEnumerable<PropertyInfo> MatchOnly() => All().Where(p => Match.Contains(p.Name));
}
