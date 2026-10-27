# Tower data schema (BTD6)

One file per monkey: `data/raw/towers/<snake_case_name>.json`, e.g. `dart_monkey.json`.
Values are game facts from the wiki, used to build a simplified simulator.
Prefer https://www.bloonswiki.com (current game version). Record the game version
the page states, if any. Costs are **Medium difficulty**. Ranges/radii in
**game units** as the wiki gives them (do not convert). Times in seconds.

```json
{
  "name": "Dart Monkey",
  "category": "primary",            // primary | military | magic | support
  "cost": 200,                      // Medium
  "footprint_radius": 6,            // placement size, if listed
  "placement": "land",              // land | water | land_or_water
  "range": 32,
  "camo": false,                    // base camo detection
  "attacks": [
    {
      "name": "dart",
      "damage": 1,
      "pierce": 2,
      "cooldown": 0.95,             // seconds between attacks
      "damage_type": "sharp",       // wiki name: sharp, explosive, normal, energy, cold, fire, plasma, glacier, shatter, ...
      "projectiles": 1,
      "blast_radius": null,         // splash radius if any
      "notes": ""                   // anything that does not fit (e.g. "seeks", "lingers 2s", "stuns 0.5s")
    }
  ],
  "abilities": [],                  // tier-3+ activated abilities: {"name","cooldown","description"}
  "upgrades": {
    "top":    [ {"tier": 1, "name": "...", "cost": 0, "description": "...", "changes": {}} ],
    "middle": [ ... 5 tiers ... ],
    "bottom": [ ... 5 tiers ... ]
  },
  "source_urls": ["..."],
  "game_version": "..."
}
```

`changes` holds the concrete stat effects of that upgrade as key/value, relative
to the previous tier where the wiki says so, e.g.
`{"pierce": "+1"}`, `{"cooldown": "x0.8"}`, `{"range": "+8"}`, `{"camo": true}`,
`{"damage_type": "normal"}`, `{"new_attack": {...attack object...}}`.
If a value is unknown or the wiki is unclear, use `null` and say so in `description`
— never guess numbers.
