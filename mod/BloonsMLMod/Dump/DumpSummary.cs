using System;
using System.Linq;
using BTD_Mod_Helper.Api.Helpers;
using Newtonsoft.Json.Linq;

namespace BloonsML.Dump;

/// <summary>
/// Kleine Übersicht über den Dump (summary.json). Wird aus den bereits serialisierten JObjects
/// gebaut statt aus den Il2Cpp-Feldern, damit sie robust gegen Feld-Umbenennungen im Spiel ist:
/// fehlt ein Feld, fehlt es nur in der Übersicht. Die vollständigen Daten stehen in den Rohdateien.
/// </summary>
public sealed class DumpSummary
{
    private static readonly string[] Difficulties = { "Easy", "Medium", "Hard", "Impoppable" };

    private static readonly string[] TowerKeys =
    {
        "name", "baseId", "tier", "tiers", "cost", "range", "towerSet", "appliedUpgrades",
        "isSubTower", "isParagon", "isBakable", "areaTypes", "radius", "footprint"
    };

    private static readonly string[] UpgradeKeys = { "name", "cost", "tier", "path", "xpCost", "isParagon", "locked" };

    private static readonly string[] BloonKeys =
    {
        "name", "baseId", "maxHealth", "speed", "leakDamage", "danger", "layerNumber",
        "isCamo", "isGrow", "isFortified", "isMoab", "isBoss", "bloonProperties", "tags",
        "distributeDamageToChildren"
    };

    private readonly string gameVersion;
    private readonly JArray towers = new();
    private readonly JArray upgrades = new();
    private readonly JArray bloons = new();
    private readonly JObject roundSets = new();
    private JArray maps = new();

    public DumpSummary(string gameVersion) => this.gameVersion = gameVersion;

    public void AddTower(JObject tower)
    {
        var entry = Pick(tower, TowerKeys);
        entry["costByDifficulty"] = CostsByDifficulty(tower["cost"]);
        towers.Add(entry);
    }

    public void AddUpgrade(JObject upgrade)
    {
        var entry = Pick(upgrade, UpgradeKeys);
        entry["costByDifficulty"] = CostsByDifficulty(upgrade["cost"]);
        upgrades.Add(entry);
    }

    public void AddBloon(JObject bloon)
    {
        var entry = Pick(bloon, BloonKeys);
        // Kinder stecken in einem SpawnChildren*-Behavior; Namen hier nur zur Übersicht
        var children = bloon.Descendants().OfType<JObject>()
            .Where(o => (o.Value<string>("$type") ?? "").Contains("SpawnChildren"))
            .Select(o => o["children"])
            .FirstOrDefault(c => c != null);
        if (children != null) entry["children"] = children.DeepClone();
        bloons.Add(entry);
    }

    public void AddRoundSet(string name, JArray rounds)
    {
        roundSets[name] = new JObject
        {
            ["rounds"] = rounds.Count,
            // null = Runde konnte nicht serialisiert werden (siehe manifest.json errors)
            ["groupsPerRound"] = new JArray(rounds.Select(r => r.Type == JTokenType.Null ? null : (JToken)((r["groups"] as JArray)?.Count ?? 0)))
        };
    }

    public void SetMapIndex(JArray mapIndex)
    {
        maps = new JArray(mapIndex.OfType<JObject>().Select(m => new JObject
        {
            ["id"] = m["id"]?.DeepClone(),
            ["difficulty"] = m["difficulty"]?.DeepClone()
        }));
    }

    public JObject ToJson() => new()
    {
        ["gameVersion"] = gameVersion,
        ["costRounding"] = "costByDifficulty berechnet mit CostHelper.CostForDifficulty aus Mod Helper " +
                           "(Medium-Preis x {0.85, 1, 1.08, 1.2}, auf 5 gerundet), nicht aus dem Spiel gelesen",
        ["upgradeTierPath"] = "upgrades[].tier und .path zählen ab 0 (tier 0 = erstes Upgrade, path 0 = oberer Pfad)",
        ["towers"] = towers,
        ["upgrades"] = upgrades,
        ["bloons"] = bloons,
        ["roundSets"] = roundSets,
        ["maps"] = maps
    };

    private static JObject Pick(JObject source, string[] keys)
    {
        var result = new JObject();
        foreach (var key in keys)
        {
            if (source.TryGetValue(key, out var value)) result[key] = value.DeepClone();
        }
        return result;
    }

    private static JToken CostsByDifficulty(JToken cost)
    {
        if (cost == null || cost.Type is not (JTokenType.Integer or JTokenType.Float)) return JValue.CreateNull();
        var medium = (int)Math.Round(cost.Value<double>());
        var result = new JObject();
        foreach (var difficulty in Difficulties)
            result[difficulty] = CostHelper.CostForDifficulty(medium, difficulty);
        return result;
    }
}
