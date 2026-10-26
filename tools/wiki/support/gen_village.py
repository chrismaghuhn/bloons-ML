import sys; sys.path.insert(0,sys.argv[1])
from common import *
C={
(1,1):{"range":"+8","note":"40 -> 48"},
(1,2):{"buff":{"target":"all towers in radius","cooldown":"x0.85","attack_speed":"~+17.6%"}},
(1,3):{"buff":{"target":"Primary towers in radius","pierce":"+1","range":"+10% (multiplicative with base village range buff)","projectile_speed":"+25%"}},
(1,4):{"range":"+7","note":"48 -> 55","buff":{"target":"Primary towers in radius","range":"+5 units (additive after Primary Training)","free_upgrades":"tier 1","ability_cooldown":"x0.85"}},
(1,5):{"buff":{"target":"Primary towers in radius","pierce":"+3 (replaces +1)","free_upgrades":"tiers 1 and 2","ability_cooldown":"x0.75 (replaces x0.85)"},
       "new_attack":atk("mega_ballista",10,100,2.5,"normal",notes="infinite range; camo detection; +270 Ceramic, +270 MOAB-class; ricochets to targets within 90 units; projectile speed 400, lifespan 2.5s, radius 11.25; MOAB pierce penalty +9. Cooldown 2.5s per main page; Stats page (v46.2) says 3s")},
(2,1):{"buff":{"target":"Regrow Bloons in radius","regrow_blocked":True}},
(2,2):{"camo":True,"buff":{"target":"all towers in radius","camo_detection":True}},
(2,3):{"buff":{"target":"all towers in radius","damage_type":"normal (can pop all Bloon types)"}},
(2,4):{"ability":"Call to Arms"},
(2,5):{"ability":"Homeland Defense","replaces_ability":"Call to Arms"},
(3,1):{"buff":{"target":"towers placed and upgrades up to tier 3 bought in radius","discount":"10%"}},
(3,2):{"buff":{"target":"towers placed and upgrades up to tier 3 bought in radius","discount":"+5% (stacks with up to 2 other Monkey Commerce villages, max 25% total)"}},
(3,3):{"buff":{"target":"all towers in radius","cash_per_pop":"+50%"}},
(3,4):{"range":"+10","buff":{"target":"income-producing towers in radius","income":"+20%"},"free_dart_monkey_per_round":True},
(3,5):{"income_per_round":None,"cost_note":"$5,000 x number of Banana Farms in range (not difficulty-dependent)","note":"sacrifices all tier<=4 Banana Farms in range; income depends on sacrificed Farm value; exact formula not given on main page"},
}
up=upgrades("Monkey_Village",C,extra_desc={(3,5):"cost field = $5,000 per Banana Farm in range (minimum one farm required); difficulty-independent."})
for u in up["bottom"]:
    if u["tier"]==5: u["cost"]=5000
obj={"name":"Monkey Village","category":"support","cost":1200,"footprint_radius":None,"placement":"land","range":40,"camo":False,
"attacks":[atk("village_aura",None,None,None,None,projectiles=0,notes={"buff":{"target":"all towers in radius","range":"+10%"},"description":"Does not attack. Base village gives +10% range to towers in its radius."})],
"abilities":[{"name":"Call to Arms","cooldown":45,"description":"0-4-0: all towers on screen get +50% pierce and +50% attack speed for 15s. Initial cooldown 15s; usable between rounds."},
             {"name":"Homeland Defense","cooldown":None,"description":"0-5-0: upgraded Call to Arms: +100% attack speed and +100% pierce for all towers for 20s. Cooldown not stated for this tier on the wiki (null); Call to Arms is 45s."}],
"upgrades":up,
"source_urls":["https://www.bloonswiki.com/Monkey_Village_(BTD6)","https://www.bloonswiki.com/Stats:Monkey_Village_(BTD6)"],
"game_version":"49.1 (main page); Stats page last updated 46.2 and is incomplete (only 0-0-0, 1-0-0, 5-0-0)"}
write("monkey_village.json",obj)
