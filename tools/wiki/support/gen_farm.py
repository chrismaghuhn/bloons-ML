import sys; sys.path.insert(0,sys.argv[1])
from common import *
C={
(1,1):{"bananas_per_round":6,"income_per_round":120},
(1,2):{"bananas_per_round":8,"income_per_round":160},
(1,3):{"bananas_per_round":16,"income_per_round":320},
(1,4):{"crates_per_round":5,"value_per_crate":300,"income_per_round":1500},
(1,5):{"crates_per_round":5,"value_per_crate":1200,"income_per_round":6000,"buff":{"target":"all Banana Research Facilities (4-x-x) on map","income":"+25%"},
       "note":"Wiki upgrade text says $7,000/round but its income table and 5 x $1,200 give $6,000; using 6000"},
(2,1):{"banana_lifespan_s":30},
(2,2):{"banana_value":"+25%","note":"e.g. 0-2-0: $25/banana, $100/round"},
(2,3):{"income_mode":"bank","bank_interest_per_round":"+15%","bank_capacity":7000,"note":"cash stored internally; withdraw any time; stops producing when full; 0-3-0 income table: $225/round"},
(2,4):{"bank_capacity":10000,"ability":"IMF Loan"},
(2,5):{"ability":"Monkey-Nomics","replaces_ability":"IMF Loan"},
(3,1):{"collect_radius":75,"expired_banana_autocollect_value":"50%"},
(3,2):{"expired_banana_autocollect_value":"85%","sellback_rate":"+10%","bank_autocollect_when_full":True},
(3,3):{"income_mode":"auto","packets_per_round":16,"value_per_packet":20,"income_per_round":320,"note":"no collectibles; path 1 crosspath adds +2 packets per tier"},
(3,4):{"value_per_packet":70,"income_per_round":1120,"buff":{"target":"all Merchantmen (Monkey Buccaneer) and above","income":"+10% each, stacks additively to max +100%"}},
(3,5):{"end_of_round_cash":4000,"end_of_round_lives":15,"range":75,"auto_collects_collectibles_in_range":True,"income_per_round":1120,"note":"income_per_round is packet income (as 0-0-4) plus $4,000 end of round"},
}
obj={"name":"Banana Farm","category":"support","cost":1250,"footprint_radius":None,"placement":"land","range":40,"camo":False,
"attacks":[atk("banana_generation",None,None,None,None,projectiles=4,
 notes={"income_per_round":80,"bananas_per_round":4,"value_per_banana":20,"banana_lifespan_s":15,"collect_radius":50,
        "description":"Does not attack. Produces bananas spread over the round (each 1/4 of the round); collectibles must be harvested. Each banana counts as an 'attack' for Overclock etc."})],
"abilities":[{"name":"IMF Loan","cooldown":85,"description":"0-4-0: gives $9,000 cash plus equal debt (repaid from income). Full initial cooldown, max 2 uses per round, disabled if round lasts >3 min, not usable between rounds or while debt outstanding."},
             {"name":"Monkey-Nomics","cooldown":60,"description":"0-5-0: replaces IMF Loan; gives $9,000 debt-free. Full initial cooldown, max 2 uses per round, disabled if round lasts >3 min, not usable between rounds."}],
"upgrades":upgrades("Banana_Farm",C),
"source_urls":["https://www.bloonswiki.com/Banana_Farm_(BTD6)"],
"game_version":"49.1 (page 'last updated' marker); no Stats page exists for Banana Farm"}
write("banana_farm.json",obj)
