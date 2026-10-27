# Design: zufällige Startzustände (abgestimmt 2026-10-27)

Ziel: Die Policy soll aus Spielständen weiterspielen können, die sie selbst nie herbeigeführt
hätte. Das ist für die Bridge entscheidend (Fehlklick, abweichendes Cash) und hilft gegen
Festhängen auf einem Rezept.

## 1. Der Zustand

Ein Startzustand ist das, was zwischen zwei Runden zählt:

```
StartState { round, cash, lives, towers: [(kind, x, y, tiers)] }   // Türme in Bau-Reihenfolge
```

- **Reihenfolge = Bau-Reihenfolge**, festgeschrieben: der Core iteriert die Türme in dieser
  Reihenfolge (Schuss-Reihenfolge, `state_hash`). Turm-IDs werden beim Aufbau neu vergeben (0, 1, …);
  ein rekonstruierter Zustand ist in sich deterministisch, hasht aber nicht zwingend wie das Original,
  wenn dort verkauft wurde.
- **Aufbau** in der Engine: Türme mit echten Preisen setzen und upgraden (damit der Verkaufswert
  stimmt), danach Cash, Leben und Runde setzen. Neu: `Env::reset_from(&StartState)`. Ungültige
  Zustände (Platz nicht bebaubar, Überlappung, Upgrade nicht erlaubt) werden abgelehnt, nicht
  repariert. Die Observation enthält Runde, Cash und Leben bereits.
- **TODO, sobald die Engine es kann:** Targeting je Turm (heute immer "First") und
  Ability-Cooldowns (Abilities kommen mit M4; Kombinationen mit Ability sind bis dahin gesperrt).

## 2. Woher die Zustände kommen

| Quelle | Anteil | Wie |
|---|---|---|
| A: eigene Rollouts, gestört | 50 % | Zustände zu Rundenbeginn aus den laufenden Trainings-Episoden |
| B: andere Läufe, gestört | 30 % | aus Replays (Baseline, Sperren-Lauf, Skript-Plan) |
| C: zufällige Builds im Budget | 20 % | in der Engine erzeugt |

- **A:** Jede Env legt Zustände zu Rundenbeginn ihrer normalen Episoden in einen kleinen Ringpuffer;
  Python sammelt sie alle paar Iterationen ein und frischt den Pool auf.
- **B:** Python liest Replays: Türme aus den Aktionen bis Runde R, Leben und Cash aus der
  Ergebniszeile von Runde R. Kein Nachsimulieren nötig.
- **C:** Budget (Cash + Turmwert) aus einer **Budgetkurve je Runde**, einmal aus den
  Baseline-Replays berechnet und zwischen den Runden interpoliert (gilt also auch für Runden ohne
  Pool-Zustände), ± Streuung; zufällig aufgeteilt in Cash und Türme (zufällige Typen, bebaubare
  Plätze, erlaubte Upgrade-Stufen im Budget). Gesperrte Typen werden nicht gewürfelt.

### Störungen (A und B), Engine würfelt beim Reset 1–2 davon

- **Cash:** zweistufig, meist (80 %) ± 5–10 %, selten (20 %) ± 30 %.
- **Leben:** in einem Drittel der Startzustände auf 50–100 % des Ursprungswerts.
- **Turm eine Stufe zurück:** die Upgrade-Kosten werden erstattet (Bridge-Fall: Klick verfehlt,
  das Geld ist noch da).
- **Turm verkaufen:** Verkaufswert wie im Spiel.
- **Turm verschieben:** auf eine freie Nachbarzelle oder eine leicht versetzte Position. Ist die neue
  Position nicht bebaubar, wird diese Störung verworfen und eine andere gewürfelt.

## 3. Wie oft und wo

- 30 % der **Episoden** starten aus einem Zustand, 70 % normal in Runde 1. Späte Starts sind kürzer;
  der Anteil an den **Trainingsschritten** wird geloggt, danach wird über die 30 % entschieden.
- Startrunde: die Runde des Pool-Zustands (A/B) bzw. gleichverteilt über 1…(Endrunde − 5) (C).
- Sperren (`--ban-towers`) bleiben unabhängig davon; ein Startzustand darf Türme eines gesperrten Typs
  enthalten (die Sperre verbietet nur Neubauten).

## 4. Hoffnungslose Zustände begrenzen

Je **Basiszustand** im Pool (vor der Störung; die Störung wird bei jedem Start neu gewürfelt, die
Statistik gilt also für den Basiszustand mit allen seinen Störungen) wird mitgezählt, ob die Episode
die nächsten 3 Runden überlebt; ein Sieg innerhalb dieser 3 Runden zählt als überlebt. Die Quote ist
ein **gleitender Mittelwert mit Zerfall**, weil die Policy besser wird. Zustände mit Quote unter 50 %
bekommen weniger Gewicht; Ziel: höchstens ~10 % "sofort verloren". Starts mit gestörten Leben werden
dabei getrennt ausgewiesen.

## 5. Logging und Messen

- Episodenmetriken (Return, erreichte Runde, Länge) getrennt nach Starttyp: normal / A / B / C, dazu
  der Anteil der Schritte je Starttyp. Vergleiche mit der Baseline nur über "normal".
- **Eval "Erholung":** fester Satz gestörter Zustände (eigener Eval-Seed, nicht aus dem Training) plus
  echte Spielstände von Hand (wenige, nur Eval, nicht Training: sie messen den Sim-to-Real-Abstand).
  Von Hand eingetragene Positionen werden gegen die Bebaubarkeit der Engine geprüft.
- Weiterhin Festhäng-Metrik und Überleben je Sperre (docs/m3-log.md).
- Vergleich gegen den Sperren-Lauf mit gleichem Entropie-Koeffizienten; der Entropie-Schedule kommt in
  einem getrennten Lauf.

## 6. Umsetzung in Schritten

1. Engine: `StartState`, `Env::reset_from`, Störungen, zufällige Builds, Ringpuffer für A,
   Starttyp/Basis-ID in den Episodeninfos. Tests.
2. Python: Pool (A/B/C), Gewichtung mit Zerfall, Logging je Starttyp, `train.py --start-states`.
3. Eval "Erholung", Lauf, Auswertung.
