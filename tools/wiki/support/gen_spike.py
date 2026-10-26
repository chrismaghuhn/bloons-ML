import sys; sys.path.insert(0,sys.argv[1])
from common import *
C={
(1,1):{"pierce":"+5","note":"5->10 (Carpet of Spikes 10->15, Perma-Spike 50->85)"},
(1,2):{"damage_type":"normal","can_pop_lead":True,"can_pop_frozen":True},
(1,3):{"replace_attack":atk("spiked_ball",2,11,1.75,"normal",notes="+3 damage to Ceramic (excl. Ceramic Armor Shell), +1 to Fortified; lifespan 50s")},
(1,4):{"replace_attack":atk("spiked_mine",2,18,1.75,"normal",notes="+6 Ceramic, +3 Fortified; lifespan 50s; explodes when depleted or expired"),
       "new_attack":atk("mine_explosion",10,30,None,"explosion",blast_radius=30,notes="+2 Ceramic, +3 Fortified; blast radius 40 with Long Reach; burn DoT 10 fire damage every 2s for 6.05s (not Purple)")},
(1,5):{"cooldown":3.5,"replace_attack":atk("super_mine",50,18,3.5,"normal",notes="+30 Ceramic, +15 Fortified; lifespan 50s"),
       "new_attacks":[atk("mini_explosion",20,30,None,"normal",blast_radius=30,notes="per spike depleted; +15 Ceramic, +5 Fortified; no burn"),
                      atk("super_explosion",1000,60,None,"normal",blast_radius=100.2,notes="on expiry; +500 Ceramic, +250 Fortified; 133 radius with 5-0-1; burn DoT 2500 fire damage every 2s for 6.05s")]},
(2,1):{"cooldown":"x0.8","note":"1.75s -> 1.4s"},
(2,2):{"cooldown":"x0.75","note":"1.4s -> 1.05s"},
(2,3):{"moab_damage":"+7"},
(2,4):{"ability":"Spike Storm"},
(2,5):{"cooldown":0.525,"range":50,"damage":"+2","pierce":"+5","note":"main spikes 3 dmg (+7 MOAB), 10 pierce; passive Spike Storm every 15s; storm spikes last 18-22s and persist 1 round",
       "new_attack":atk("carpet_of_spikes_passive",3,10,15,"sharp",projectiles=204,notes="passive Spike Storm every 15s: 4 piles every 0.04s for 2s (~204 piles) randomly over all track; +7 MOAB damage; lifespan 18-22s; persists 1 round")},
(3,1):{"range":"+8","spike_lifespan":100,"note":"range 34->42, lifespan 50->100s; mines' blast radius ~+33%"},
(3,2):{"targeting_options":["Normal","Close","Smart","Automatic","Set Target"],"buff":{"start_of_round_cooldown":"x0.25","duration_s":3}},
(3,3):{"spike_lifespan":140,"persists_rounds":1},
(3,4):{"damage":3,"persists_rounds":2},
(3,5):{"cooldown":6.0625,"damage":10,"pierce":50,"spike_lifespan":300,"persists_rounds":3,"buff":{"start_of_round_cooldown":"x0.25","duration_s":10}},
}
obj={"name":"Spike Factory","category":"support","cost":1000,"footprint_radius":8,"placement":"land","range":34,"camo":False,
"attacks":[atk("spike_pile",1,5,1.75,"sharp",notes="Places spike pile on track within range; lifespan 50s or end of round; pile radius 6; cannot target Camo; cannot pop Lead or Frozen (they destroy the pile)")],
"abilities":[{"name":"Spike Storm","cooldown":50,"description":"Tier 2-4 (middle). Clusters of 4 spike piles every 0.04s for 2s (~204 piles) randomly across all track; same spike stats as main attack (1 dmg, 5 pierce, +7 MOAB at 0-4-0); spikes last 9-11s and persist 1 round."}],
"upgrades":upgrades("Spike_Factory",C),
"source_urls":["https://www.bloonswiki.com/Spike_Factory_(BTD6)","https://www.bloonswiki.com/Stats:Spike_Factory_(BTD6)"],
"game_version":"55.1 (stats page last updated); main page last updated 55.2"}
obj["abilities"][0]["description"]=obj["abilities"][0]["description"].replace("Tier 2-4 (middle)","Granted by Spike Storm (0-4-0); kept at 0-5-0")
write("spike_factory.json",obj)
