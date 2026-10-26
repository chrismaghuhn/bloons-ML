import json, sys
cargo = json.load(open(sys.argv[1]))
ups = {(r["title"]["path"], r["title"]["tier"]): r["title"] for r in cargo["cargoquery"]}
def U(p, t, changes, note=""):
    r = ups[(str(p), str(t))]
    d = r["description"] + ((" [Wiki details, paraphrased: " + note + "]") if note else "")
    return {"tier": t, "name": r["name"], "cost": int(r["cost"]), "description": d, "changes": changes}
SRC = "Stats for upgrades come from the wiki's upgrade prose (no structured stats module for Skywarden yet)."
res = {
 "name": "Skywarden", "category": "magic", "cost": 205, "footprint_radius": 6, "placement": "land",
 "range": 48, "camo": False,
 "attacks": [{"name": "Attack", "damage": 1, "pierce": 4, "cooldown": 1.55, "damage_type": "sharp",
   "projectiles": 1, "blast_radius": None,
   "notes": "arrow; random spread 12 deg; speed 450 decelerating (-20 units/s^2); max travel 75 units; lifespan 0.16s; "
            "projectile radius 2; self attack-speed buff +5% per damaging hit, max 10 stacks (+50%), resets after 2s without dealing damage "
            "(multi-damage hit counts once)"}],
 "abilities": [{"name": "Thunder's Decree (Thunder Charge)", "cooldown": 45,
   "description": "Towers in range get +200 projectile speed, can pop Frozen, projectiles ricochet twice (to Bloons within 100 units, 0.05s after hit; already-ricocheting projectiles get +2 bounces). Duration 9s. Cooldown becomes 30s with Stormwrath Archon, which also adds a lightning bolt every 0.25s (40 damage, 50 pierce, 35% chance to stun 2s) jumping between lightning rods. [unlocked by middle path tier 4; values from wiki upgrade text]"}],
 "upgrades": {
  "top": [
   U(1, 1, {"range": "+10", "buff_nearby_range": "+3 per stack, max 3 stacks (+9), to towers up to 1 tier above this Skywarden's highest tier (up to tier 5 once it has a tier 4+ upgrade)"}),
   U(1, 2, {"camo": True, "long_range_stance": "toggle (Monkey Special, ~5s cooldown): cooldown x1.25, range x1.35, projectile lifespan x1.1, no deceleration, homing on target; spread unchanged"}),
   U(1, 3, {"wind_combo": "mark on damaged Bloons lasts 3s, re-hit advances combo (up to stage 3); stage 1: first Bloon hit by each arrow is blown back 10-20 units every 0.5s while marked (not MOABs); stage 2 (3rd hit): each arrow spawns 2 extra arrows 90 deg apart with same damage/pierce; stage 3 (4th hit): vortex on path, 4 pierce, 20-40 units blowback, lasts up to 0.8333s"}),
   U(1, 4, {"damage": "+1", "projectiles_per_attack": "burst of 3 arrows, 0.15s apart", "cooldown": None,
            "camo_disable_zone": "disables Camo property of Bloons while inside its attack range (not permanent)"},
     "arrows deal 2 damage each; the cooldown between bursts is not stated on the wiki page, so cooldown is null"),
   U(1, 5, {"damage": "+3", "range_damage_bonus": "+1 damage at 70 range, then +1 per 5 range above 70, cap +20", "wind_combo_stage2_extra_arrows": 4},
     "base arrow damage becomes 5; in long-range stance with no other buffs it deals 7"),
  ],
  "middle": [
   U(2, 1, {"damage_type": "energy", "projectile_speed": "x1.5", "max_attack_speed_bonus": "+100% (was +50%), reached after 20 shots"}),
   U(2, 2, {"new_attack": {"name": "Arc blast", "damage": 1, "pierce": 4, "cooldown": 1.55, "damage_type": "plasma", "projectiles": 1,
             "blast_radius": 10, "notes": "arrow now produces a delayed blast at the target; +1 pierce and +1 blast radius per 10 units travelled; ignores obstacles; gains Set Target targeting. Cooldown assumed same as main attack (not restated on wiki)"}}),
   U(2, 3, {"damage_bonus": "+3 vs MOAB-class, Ceramic, Lead (non-stacking)",
            "lightning_combo": "mark on one Bloon at a time, 3s; marked Bloons attract energy/plasma projectiles within 30 units; stage 2: sharp/shatter hits on marked Bloon create chain lightning (4 pierce, splits once, 3 damage, +4 vs frozen); stage 4 (4th hit): arrow +10 damage and stuns 3s (1s MOAB/DDT, 0.5s BFB, not ZOMG)"}),
   U(2, 4, {"combo_stun": "MOAB/DDT 2s, BFB 1s, ZOMG 0.5s", "Arc blast.pierce_per_10_units": "+1.5 (was +1)",
            "new_ability": {"name": "Thunder Charge", "cooldown": 45, "effect_stats": "9s duration; see abilities"}}),
   U(2, 5, {"damage_bonus": "+9 vs MOAB-class, Ceramic, Lead (was +3)", "combo_lightning_damage": 50, "combo_stun_extra_damage": 100,
            "new_attack": {"name": "Lightning rod", "damage": 40, "pierce": 5, "cooldown": 0.5, "damage_type": None, "projectiles": 1, "blast_radius": 26,
              "notes": "rods placed on track near target every 3s, last 12s, min 10 units apart; zap when a Bloon within 26 units is hit by energy/plasma; max pierce 5; damage type not stated on wiki"},
            "ability.Thunder Charge.cooldown": 30}),
  ],
  "bottom": [
   U(3, 1, {"pierce": "+2", "spread": "x0.25 (75% less)", "frozen": "ignores frozen immunity (still sharp); instantly pops frozen Bloons Pink or lower"}),
   U(3, 2, {"damage_vs_frozen": "+1", "self_buff": "popping a frozen Bloon gives +50% projectile pierce for 1s (not refreshed while active)"}),
   U(3, 3, {"slow": "first Bloon hit by each arrow slowed 25% for 3s (not MOABs)",
            "ice_combo": "mark 3s on damaged Bloons; marked Bloons consume 1 less pierce from all projectiles; stage 3: 20% slow to other Bloons within 8 units; stage 4: 2 shrapnel to nearest Bloons within 300 units, 3 pierce, 3 frigid damage, +3 vs frozen, 35% chance to freeze non-MOABs 1.5s"}),
   U(3, 4, {"damage": "+1", "damage_vs_frozen": "+4 (was +1)", "shrapnel_damage": "6 (+6 vs frozen)",
            "icesplosive_remains": "on popping a frozen Bloon: remain lasts 2-4s, slows up to 6 Bloons (incl. MOABs) 50% for 3s, then explodes: 3 pierce, 3 frigid damage, radius 12, freezes 6 non-MOAB layers 1s",
            "moab_slow": "combo slows MOAB-class 10% (+5% area); shrapnel can freeze MOABs"}),
   U(3, 5, {"damage": "+2", "damage_vs_frozen": "+8 (was +4)", "shrapnel_bonus_vs_frozen": "+12",
            "frozen_count_bonus": "+1 damage per 3 frozen Bloons in range (cap +8); +1 damage for 6s per 3 frozen Bloons popped",
            "icesplosive_remains": "radius 24, damage 10; bonus from popped MOABs: MOAB +50, BFB +150, DDT +100, ZOMG +400, BAD +10000"}),
  ],
 },
 "source_urls": ["https://www.bloonswiki.com/Skywarden_(BTD6)"],
 "game_version": "56.3",
}
res["attacks"][0]["notes"] += "; " + SRC
json.dump(res, open(sys.argv[2], "w"), indent=2, ensure_ascii=False)
