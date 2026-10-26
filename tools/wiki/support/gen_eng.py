import sys; sys.path.insert(0,sys.argv[1])
from common import *
sentry=atk("sentry_nail",1,2,0.95,"sharp",notes="sub-tower Sentry: range 45, lifespan 25s, footprint 4; cannot target Camo")
C={
(1,1):{"new_attack":atk("sentry_spawner",None,None,10,None,notes="spawns a Sentry at random position within 40 range every 10s; Sentry: 1 dmg sharp, 2 pierce, 0.95s cooldown, range 45, lifespan 25s"),"sub_tower":sentry},
(1,2):{"sentry_spawn_cooldown":5,"note":"spawner cooldown 10s -> 5s; also halves foam/trap cooldowns when crosspathed"},
(1,3):{"cooldown":0.35,"sentry_cooldown":0.475,"note":"nailgun and new sentries attack speed doubled"},
(1,4):{"replace_sub_tower":[
   atk("crushing_sentry",2,22,1.2,"shatter",notes="range 50, +4 Ceramic damage, lifespan 25s"),
   atk("boom_sentry",4,30,1.0,"explosion",blast_radius=18,notes="range 50; explosion on impact; lifespan 25s"),
   atk("cold_sentry",1,24,1.5,"cold",blast_radius=18,notes="range 50; freezes 1.5s (2 layers), permafrost x0.5 speed; cannot target MOAB-class or frozen; lifespan 25s"),
   atk("energy_sentry",2,8,0.57,"energy",notes="range 50; +2 MOAB-class damage; lifespan 25s")],
   "note":"Sentry type chosen based on Bloons on screen (default order Crushing, Boom, Cold, Energy)"},
(1,5):{"cooldown":0.3,"replace_sub_tower":atk("champion_sentry",3,5,0.06,"plasma",notes="range 50, lifespan 25s; on expiry self-destructs: 100 dmg plasma, 50 pierce, radius 50")},
(2,1):{"range":"+20","note":"40 -> 60; sentries +4 range when crosspathed"},
(2,2):{"moab_damage":"+1","fortified_damage":"+1"},
(2,3):{"new_attack":atk("cleansing_foam",None,10,2.0,None,projectiles=4,notes="targeted anywhere in range (locked); foam splat radius 7, lasts 8.5s, 10 pierce; removes Camo, Regrow, and Lead layer (not DDT); no damage"),
       "buff":{"start_of_round_cooldown":"x0.25","duration_s":1}},
(2,4):{"pierce":15,"note":"main pierce +12 (=15), foam pierce 14, nail speed x2 lifespan x0.5","ability":"Overclock"},
(2,5):{"pierce":30,"note":"main pierce 30, foam pierce 24","ability":"Ultraboost","replaces_ability":"Overclock"},
(3,1):{"pierce":"+5","damage_type":"shatter","note":"pierce 3 -> 8; can pop frozen; sentries +1 pierce & shatter if crosspathed"},
(3,2):{"stun":{"duration_s":1,"applies_to":"non-MOAB","note":"pins next layer"}},
(3,3):{"cooldown":0.35,"note":"double gun alternating; attack speed x2"},
(3,4):{"new_attack":atk("bloon_trap",None,None,12,None,notes="trap placed on nearest track; captures up to 500 RBE then collect for cash with +100% cash from captured Bloons; no MOAB-class, no Camo unless detected; radius 10")},
(3,5):{"trap_cooldown":4.6,"trap_capacity_rbe":10000,"trap_cash_bonus":"+200%","note":"XXXL trap can capture MOAB-class except BADs and Bosses"},
}
obj={"name":"Engineer Monkey","category":"support","cost":350,"footprint_radius":7,"placement":"land","range":40,"camo":False,
"attacks":[atk("nail",1,3,0.7,"sharp",notes="projectile speed 375, lifespan 0.24s, radius 2; cannot target Camo; cannot pop Lead or Frozen")],
"abilities":[{"name":"Overclock","cooldown":45,"description":"0-4-0: target tower gets cooldown x0.6 (+66.67% attack speed); Villages get x1.1 range. Duration 30-60s depending on target's highest tier (30s on heroes)."},
             {"name":"Ultraboost","cooldown":35,"description":"0-5-0: Overclock effect for 60s on all towers plus permanent stacking buff: cooldown x0.96 per use (max 10 stacks; Villages range x1.015 per stack). Max 10 uses per round per Engineer."}],
"upgrades":upgrades("Engineer_Monkey",C),
"source_urls":["https://www.bloonswiki.com/Engineer_Monkey_(BTD6)","https://www.bloonswiki.com/Stats:Engineer_Monkey_(BTD6)"],
"game_version":"55.1 (Stats page last updated)"}
write("engineer_monkey.json",obj)
