import sys; sys.path.insert(0,sys.argv[1])
from common import *
SP="Stats-page value (v55.2); "
C={
(1,1):{"range":60,"replace_attack":atk("piranha",1,1,0.6,"shatter",blast_radius=4,notes="beast range 25 per main page text, Stats page says 30; requires water within 60 units; beast power 1")},
(1,2):{"replace_attack":atk("barracuda",2,4,0.6,"shatter",blast_radius=8,notes="beast range 35; knockback 50% (25% Lead/Ceramic) for 0.2s; beast power 3-6: damage 2-4, pierce 4-12 across power")},
(1,3):{"replace_attack":atk("great_white_thrash",12,10,0.6,"normal",blast_radius=15,notes="beast range 40; jump attack cooldown 1.1s, thrashes every 0.6s while latched (up to 3s); grabs Bloons incl. Lead; drags one-shottable targets; MOAB at max power; beast power 8-24: damage 12-36, pierce 10-30")},
(1,4):{"replace_attack":atk("orca_thrash",30,20,0.6,"normal",blast_radius=24,notes="beast range 50; jump cooldown 1.1s; drags MOAB/BFB (ZOMG/DDT at max power); stronger knockback; beast power 16-64: damage 30-90, pierce 20-60")},
(1,5):{"replace_attack":atk("megalodon_thrash",600,50,0.6,"normal",blast_radius=48,notes="beast range 60; jump cooldown 1.1s; drags MOAB-class incl. BAD (max 30s); requires 3 extra merged Orcas (power 64); beast power 36-132: damage 600-1800, pierce 50-90")},
(2,1):{"range":60,"replace_attack":atk("microraptor",1,5,1.0,"sharp",blast_radius=10,notes="beast range 20; beast power 1")},
(2,2):{"replace_attack":atk("adasaurus",2,8,1.0,"normal",blast_radius=15,notes="beast range 20; +1 Ceramic damage; beast power 3-6: damage 2-4, pierce 8-20")},
(2,3):{"replace_attack":atk("velociraptor",4,12,1.0,"normal",blast_radius=18,notes="beast range 24; +4 Ceramic, +3 to stunned, x1.5 vs stunned; beast power 8-24: damage 4-16, pierce 12-26, cooldown 1.0-0.6561s")},
(2,4):{"replace_attack":atk("t_rex",20,22,1.0,"normal",blast_radius=18,notes="beast range 30; +5 Ceramic, +8 to stunned (main page: +8-21 over power); beast power 16-64: damage 20-60, pierce 20-45 (Stats page lists 22), cooldown 1.0-0.6561s; at power 64 damages through MOAB layers"),"ability":"Stomp"},
(2,5):{"replace_attack":atk("giganotosaurus",750,44,1.25,"normal",blast_radius=24,notes=SP+"main page gives power range 36-132: damage 500-2000 (1250 at 84), pierce 33-83 (58 at 84), +250-516 to stunned, cooldown 1.25-0.9061s; beast range 50; ignores blockers; damage soaks through MOAB layers; requires 3 extra merged T-Rexes"),"ability":"Stomp (Giganotosaurus)"},
(3,1):{"range":60,"replace_attack":atk("gyrfalcon",1,3,1.0,"sharp",blast_radius=10,notes="flying beast: range 60, flight speed 80; grabs Bloons past reticle and carries them back toward drop-off point, scratches those before it; +1 damage to Regrow; cannot target Ceramic+ or Lead; popped Bloons lose children; pierce/attack rate unbuffable")},
(3,2):{"camo":True,"replace_attack":atk("horned_owl",1,6,1.0,"sharp",blast_radius=20,notes="camo detection; can grab Ceramics (+4 pierce penalty, 1-3 at a time); Lead +1 pierce penalty; merges increase attack speed, pierce and flight speed (Golden Eagle text implies owl pierce range 6-18, damage 1-2); damage 2 at max power")},
(3,3):{"replace_attack":atk("golden_eagle",1,30,1.0,"sharp",blast_radius=25,notes="Ceramic pierce penalty +1; no Lead penalty; at power 24 can grab MOAB (pierce penalty +1089); beast power 8-24: pierce 30-90, damage 1-3")},
(3,4):{"replace_attack":atk("giant_condor",2,30,1.0,"sharp",blast_radius=25,notes="+11 MOAB-class damage; grabs MOAB (+14 pierce penalty), BFB (+29), DDT (+29, needs lead popping); beast power 16-64: damage 2-6; cannot target Lead/DDT without lead popping")},
(3,5):{"replace_attack":atk("pouakai",10,150,0.8,"normal",blast_radius=45,notes="+80 MOAB-class damage; grabs MOAB(+14)/BFB(+29)/DDT(+29)/ZOMG(+59 pierce penalty); can damage but not grab BAD/Boss; beast power 36-132: damage 10-90, pierce 150-450, cooldown 0.8-0.28s; requires 3 extra merged Condors")},
}
obj={"name":"Beast Handler","category":"support","cost":250,"footprint_radius":7,"placement":"land","range":20,"camo":False,
"attacks":[atk("staff",1,4,1.4,"energy",blast_radius=12,notes="short range staff stab; can pop frozen but not Lead or Purple (Energy); ignores line of sight. Upgrades replace this attack with a Beast sub-tower whose stats scale with Beast Power (merging handlers adds power). Handler can hold beasts from more than one path. Beast Power base/max per tier: T1 1/1, T2 3/6, T3 8/24, T4 16/64, T5 36/132 (T5 requires power 64 at T4). Stat at power P = base + floor(range * (P-min)/(max-min)).")],
"abilities":[{"name":"Stomp","cooldown":40,"description":"0-4-0 T-Rex: 100 radius blast, 150 damage, 400 pierce (150-202 dmg, 400-433 pierce with power); stuns Bloons 6s, MOAB/BFB 3s, ZOMG/DDT 2s (not BAD/Boss). Cooldown 40s down to 19.03s at max power."},
             {"name":"Stomp (Giganotosaurus)","cooldown":25,"description":"0-5-0: whole screen (radius 2000), 300 damage, 600 pierce (300-1100 dmg, 600-690 pierce with power); stuns Bloons 12s, MOAB 8s, BFB 6s, ZOMG/DDT 4s. Cooldown 25s down to 15.51s at max power."}],
"upgrades":upgrades("Beast_Handler",C,extra_desc={(1,5):"Requires 3 additional merged tier-4 Orca handlers.",(2,5):"Requires 3 additional merged tier-4 T-Rex handlers.",(3,5):"Requires 3 additional merged tier-4 Condor handlers."}),
"source_urls":["https://www.bloonswiki.com/Beast_Handler_(BTD6)","https://www.bloonswiki.com/Stats:Beast_Handler_(BTD6)"],
"game_version":"55.2 (Stats page last updated)"}
write("beast_handler.json",obj)
