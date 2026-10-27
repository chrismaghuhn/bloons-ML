# Normalisierungs-Report

Erzeugt von `tools/normalize.py`. Nicht von Hand bearbeiten.

| Turm | Version | Angriffe (Basis) | unparsed | fehlend | nicht modelliert |
|---|---|---|---|---|---|
| alchemist | 55.1 | 1 | 0 | 0 | 2 |
| banana_farm | 49.1 (page 'last updated' marker); no Stats page exists for Banana Farm | 1 | 0 | 1 | 43 |
| beast_handler | 55.2 (Stats page last updated) | 1 | 0 | 0 | 2 |
| bomb_shooter | stats last updated for 56.3 per wiki Stats page; latest BTD6 version on wiki: 57.4 | 1 | 0 | 0 | 3 |
| boomerang_monkey | stats last updated for 55.1 per wiki Stats page; latest BTD6 version on wiki: 57.4 | 1 | 0 | 0 | 2 |
| dart_monkey | stats last updated for 55.1 per wiki Stats page; latest BTD6 version on wiki: 57.4 | 1 | 0 | 0 | 3 |
| dartling_gunner | 55.1 | 1 | 0 | 0 | 3 |
| desperado | stats last updated for 55.2 per wiki Stats page; latest BTD6 version on wiki: 57.4 | 1 | 0 | 0 | 4 |
| druid | 55.1 | 1 | 0 | 0 | 16 |
| engineer_monkey | 55.1 (Stats page last updated) | 1 | 0 | 0 | 24 |
| glue_gunner | stats last updated for 55.1 per wiki Stats page; latest BTD6 version on wiki: 57.4 | 1 | 0 | 0 | 2 |
| heli_pilot | 55.1 | 1 | 0 | 1 | 11 |
| ice_monkey | stats last updated for 55.1 per wiki Stats page; latest BTD6 version on wiki: 57.4 | 1 | 0 | 0 | 6 |
| mermonkey | 55.1 | 2 | 0 | 0 | 6 |
| monkey_ace | 55.1 | 1 | 0 | 1 | 4 |
| monkey_buccaneer | 55.1 | 1 | 0 | 0 | 12 |
| monkey_sub | 55.1 | 1 | 0 | 0 | 6 |
| monkey_village | 49.1 (main page); Stats page last updated 46.2 and is incomplete (only 0-0-0, 1-0-0, 5-0-0) | 1 | 0 | 1 | 20 |
| mortar_monkey | 55.1 | 1 | 0 | 0 | 3 |
| ninja_monkey | 55.1 | 1 | 0 | 0 | 3 |
| skywarden | 56.3 | 1 | 0 | 1 | 32 |
| sniper_monkey | 55.1 | 1 | 0 | 0 | 5 |
| spike_factory | 55.1 (stats page last updated); main page last updated 55.2 | 1 | 0 | 0 | 18 |
| super_monkey | 55.1 | 1 | 0 | 0 | 8 |
| tack_shooter | stats last updated for 55.1 per wiki Stats page; latest BTD6 version on wiki: 57.4 | 1 | 0 | 0 | 3 |
| wizard_monkey | 56.1 | 1 | 0 | 0 | 3 |

## Schema

- alle Dateien gültig

## Schadenstypen

- `sharp`: 220
- `normal`: 98
- `explosive`: 58
- `shatter`: 36
- `plasma`: 26
- `energy`: 23
- `acid`: 21
- `fire`: 18
- `frigid`: 14
- `cold`: 11
- `glacier`: 3

## Nicht modellierte Schlüssel (stehen in `effects`)

- `note`: 22
- `buff:*`: 20
- `new_ability`: 18
- `buff`: 16
- `abilities`: 16
- `income_per_round`: 9
- `ability`: 9
- `changed_ability`: 7
- `movement_speed`: 6
- `zone:*`: 5
- `targeting_options`: 4
- `new_targeting_options`: 4
- `passive_effects`: 4
- `ability.Ability.effect_stats`: 4
- `bananas_per_round`: 3
- `replaces_ability`: 3
- `cash_per_round`: 3
- `damage_vs_frozen`: 3
- `spike_lifespan`: 3
- `persists_rounds`: 3
- `ability.Ability.cooldown`: 2
- `crates_per_round`: 2
- `value_per_crate`: 2
- `income_mode`: 2
- `bank_capacity`: 2
- `expired_banana_autocollect_value`: 2
- `value_per_packet`: 2
- `replace_sub_tower`: 2
- `moab_damage`: 2
- `damage_bonus`: 2
- `icesplosive_remains`: 2
- `sacrifices`: 2
- `banana_lifespan_s`: 1
- `banana_value`: 1
- `bank_interest_per_round`: 1
- `collect_radius`: 1
- `sellback_rate`: 1
- `bank_autocollect_when_full`: 1
- `packets_per_round`: 1
- `end_of_round_cash`: 1
- `end_of_round_lives`: 1
- `auto_collects_collectibles_in_range`: 1
- `ability.Passive Ability.effect_stats`: 1
- `ability.Vine Crush.effect_stats`: 1
- `cashPerRound`: 1
- `attack_speed_bonus`: 1
- `damageBasedAttackSpeedDamageThreshold`: 1
- `damageBasedAttackSpeedIncreasePerThreshold`: 1
- `damageBasedAttackSpeedMaxStacks`: 1
- `damageBasedAttackSpeedMaxTimeWithoutDamage`: 1
- `vengeanceRbeThreshold`: 1
- `vengeanceDamage`: 1
- `vengeanceMaxDamageBoost`: 1
- `sub_tower`: 1
- `sentry_spawn_cooldown`: 1
- `sentry_cooldown`: 1
- `fortified_damage`: 1
- `stun`: 1
- `trap_cooldown`: 1
- `trap_capacity_rbe`: 1
- `trap_cash_bonus`: 1
- `free_dart_monkey_per_round`: 1
- `cost_note`: 1
- `buff_nearby_range`: 1
- `long_range_stance`: 1
- `wind_combo`: 1
- `projectiles_per_attack`: 1
- `camo_disable_zone`: 1
- `range_damage_bonus`: 1
- `wind_combo_stage2_extra_arrows`: 1
- `projectile_speed`: 1
- `max_attack_speed_bonus`: 1
- `lightning_combo`: 1
- `combo_stun`: 1
- `Arc blast.pierce_per_10_units`: 1
- `combo_lightning_damage`: 1
- `combo_stun_extra_damage`: 1
- `ability.Thunder Charge.cooldown`: 1
- `spread`: 1
- `frozen`: 1
- `self_buff`: 1
- `slow`: 1
- `ice_combo`: 1
- `shrapnel_damage`: 1
- `moab_slow`: 1
- `shrapnel_bonus_vs_frozen`: 1
- `frozen_count_bonus`: 1
- `can_pop_lead`: 1
- `can_pop_frozen`: 1
- `footprint`: 1

## Nicht interpretierbare Werte


## Fehlende Werte (null im Wiki)

- Banana Farm: footprint unbekannt (null)
- Heli Pilot: footprint unbekannt (null)
- Monkey Ace: footprint unbekannt (null)
- Monkey Village: footprint unbekannt (null)
- skywarden top4 (Galesage) cooldown: Wert im Wiki unbekannt (null)
