# Phase E.8 — Addendum : vanilla 1.8.9 à FPS non plafonné

Date : 2026-11-13. Complète la limite 3 de `phase_e8_teleport_sync_investigation.md`. Aucun code modifié
(deux modifications de l'agent, un minuteur d'arrêt, ont été refusées par le garde et **n'ont pas été appliquées**).

**Résultat : à FPS non plafonné, le client vanilla 1.8.9 est lui aussi expulsé par le serveur
(`Sent too many packets.`) dans la boucle de téléportation d'un joueur mort.** La prédiction de E.8 §7 est confirmée :
la cadence de réponse de vanilla dépend du FPS, et sans plafond elle dépasse la limite de 500 paquets par seconde.

## Mesure (expérience X3 : joueur mort à terre, puis `tp`), client vanilla seul

Même serveur et même procédure que E.8 (`xwatch.sh X3`) ; seule la valeur `maxFps` de `options.txt` change
(`enableVsync:false`). La valeur 60 a été restaurée à la fin (vérifié). Fichiers : `phase_e8_data/uncapped_fps_*.txt`.

| `maxFps` | Boucle S08/C06 | Pic de S08 par seconde | Issue |
|---|---|---|---|
| 60 (E.8) | oui | 61 | jamais expulsé (7 min) |
| 120 | **non** (8 S08 au total) | 7 | session normale jusqu'à la fin |
| **260 (« Unlimited »)** | **oui, 3 472 S08 / 3 472 C06** | **798** | **expulsé : `Fp260 lost connection: Sent too many packets.`** (5 s après le `tp`) |

Détail du run à 260 FPS (S08 / total de paquets client par seconde, à partir de la 6e seconde) :
`6 s : 90 / 90`, `7 s : 678 / 678`, `8 s : 727 / 728`, `9 s : 771 / 771`, `10 s : 798 / 799`, `11 s : 406 / 407`.
L'écart médian entre deux S08 est de 1 ms. Umicraft (E.8) : environ 3 360 S08 par seconde ; vanilla non plafonné :
environ 800. **Même comportement qualitatif**, ordre de grandeur différent, tous deux au-dessus du seuil de 500/s.

## Ce que cela établit

1. La divergence d'Umicraft avec vanilla n'est **pas qualitative** : vanilla non plafonné s'expulse de la même façon.
   Elle est **quantitative** : la cadence vanilla est bornée par le FPS (60 FPS : ~61/s ; non plafonné : ~800/s, limité
   par le coût d'une image), celle d'Umicraft ne l'est par rien (~3 360/s).
2. Le serveur de test reste le déclencheur : la boucle n'apparaît que pour un joueur mort, et **pas à chaque fois**.
   À 120 FPS elle ne s'est pas déclenchée (un run) alors qu'elle l'a fait à 60 et à 260. Le déclencheur dépend donc d'un
   minutage non maîtrisé (course côté serveur ?), non isolé.

## Limites

- Un run par FPS ; l'absence de boucle à 120 FPS n'est pas répétée (hasard ou dépendance au FPS ?).
- Seule la valeur `maxFps` change ; le FPS réellement atteint à 260 n'est pas mesuré directement (déduit de la cadence
  des S08, ~800 par seconde au pic).
- Machine et scène vides : un autre matériel donnerait une autre cadence non plafonnée.
- **Un client vanilla de test est resté ouvert** (écran « Disconnected » après l'expulsion), faute du minuteur
  d'arrêt de l'agent (modification refusée). Il doit être fermé à la main.

## Conséquence pour une correction (rappel de E.8 §13)

Une correction fidèle ne peut pas viser « la cadence de vanilla » : elle varie de ~60/s à ~800/s selon le FPS, et
vanilla s'expulse lui-même au-dessus de 500/s. Il faut donc d'abord choisir ce que l'on reproduit : (a) la cadence par
itération de boucle d'affichage, avec le même risque que vanilla non plafonné, ou (b) un garde-fou d'Umicraft
(plafond de réponses par seconde), qui est un écart **volontaire** par rapport à vanilla, à documenter comme tel.
Aucune correction n'est appliquée.
