# Phase E.7 — Mesure des cas aquatiques restants contre vanilla 1.8.9

Date : 2026-11-13. **Mesures uniquement, aucune modification du gameplay.** Le code de `umicraft-game` est celui
de E.6. Seuls des scénarios d'entrée ont été ajoutés (`e4` dans `e1.rs`, `headless.rs`, `args.rs`, `e4.txt`).

**Résultat : les six configurations mesurées sont conformes à vanilla** (écart de position ≤ 1,4e-6 bloc, de
mouvement ≤ 5e-8, drapeaux identiques) : eau sans enchantement, Depth Strider I, II et III au fond du bassin, et nage
en surface (`onGround` faux) avec Depth Strider 0, I et III. Aucun nouvel écart de comportement n'a été trouvé.
Deux anomalies d'**harnais** (pas de gameplay) et une anomalie d'Umicraft non expliquée sont documentées au §5.

## 1. Montage

PandaSpigot 225 de test, `online-mode=false`, monde plat, JAR 1.8.9 SHA-1
`3870888a6c3d349d3771a3e9d16c9bf5e076b908`, agent Java (E.2), Umicraft corrigé (E.4 + E.6).
`tp 0.5 4 0.5 0 0`, puis `clear`, bassin d'eau (`fill -5 Y 4 5 3 30 water`) et bottes
`diamond_boots` `{ench:[{id:8,lvl:N}]}` (rien pour N = 0), envoyés par console pour **les deux clients**
(`tpwatch3.sh`). Un joueur neuf par configuration et par client.

| Scénario | Phases (ticks) | Entrées |
|---|---|---|
| `e3` (fond du bassin) | `approach` 15, `swim_walk` 40, `swim_sprint` 40, `swim_stop` 20 | W, puis W + sprint |
| `e4` (nage en surface) | `approach` 15, `surface_walk` 40, `surface_sprint` 40, `surface_stop` 20 | W, puis **W + saut tenu** (le saut maintient le nageur à flot), puis W + saut + sprint |

Dans `e4` le bassin est plus profond (y 1..3). Contrôle sur le journal vanilla de `e4_deep3` : 40/40 ticks de
`surface_walk` et de `surface_sprint` avec `onGround` **faux**, y entre 1,25 et 5,25 : la branche `!onGround` (niveau
divisé par 2) est bien exercée. Dans `e3`, `swim_sprint` a 0 tick en l'air (le joueur est sur le fond).

## 2. Résultats (comparaison stricte vanilla / Umicraft, un run par côté)

Écarts maximum par phase ; drapeaux (`onGround`, sprint, sneak, touches), yaw et pitch : 0 écart.
Fichiers : `reports/phase_e7_data/state-diff-*.txt`.

| Configuration | Phase de sprint : `max|dz|` / `max|dm|` | Phase de marche : `max|dz|` / `max|dm|` | Verdict |
|---|---|---|---|
| `e3` bottes 0 (eau sans enchantement) | 4,4e-7 / 2,7e-9 | 5,7e-7 / 1,5e-8 | conforme |
| `e3` Depth Strider I | 5,9e-7 / 1,7e-8 | 6,0e-7 / 1,5e-8 | conforme |
| `e3` Depth Strider II | 9,9e-7 / 2,2e-8 | 5,9e-7 / 1,5e-8 | conforme |
| `e3` Depth Strider III (E.6, rappel) | 1,0e-6 / 1,9e-8 | 5,7e-7 / 1,5e-8 | conforme |
| `e4` surface, bottes 0 | 1,7e-7 / 1,0e-8 | 4,7e-7 / 5,0e-8 | conforme |
| `e4` surface, Depth Strider I | 2,1e-7 / 1,4e-8 | 4,7e-7 / 5,0e-8 | conforme |
| `e4` surface, Depth Strider III | 1,3e-6 / 2,9e-8 | 4,7e-7 / 5,0e-8 | conforme |

Pour mémoire, E.5 avait mesuré **avant** correction 6,5e-2 / 1,6e-2 en Depth Strider III au fond.

**Les mesures sont sensibles au facteur de sprint.** Au premier tick de `swim_sprint` (fond du bassin, vanilla) la
vitesse `mz` change selon le niveau : bottes 0 : 0,0784, inchangée par rapport à la fin de `swim_walk` (0,0784) ;
Depth Strider I : 0,1219 (0,1149 à la fin de la marche) ; Depth Strider II : 0,1351. Umicraft suit vanilla à ces valeurs
dans l'écart ci-dessus. Cela confirme la correction E.6 pour ces niveaux ; en surface, les trois cas sont conformes
sur toute la durée.

## 3. Ce que ces mesures valident, et ce qu'elles ne valident pas

**Validé sur serveur réel contre vanilla 1.8.9** : eau avec sprint pour Depth Strider 0, I, II, III joueur au fond
(`onGround` vrai) ; nage en surface (`onGround` faux, saut tenu) avec Depth Strider 0, I, III.

**Pas validé :**
1. Depth Strider II en surface, et la nage en eau libre sans saut tenu ; une seule trajectoire par cas.
2. **Avant/après pour la nage en surface** : je n'ai pas rejoué ces cas avec l'ancien code, donc je ne peux pas dire
   qu'ils auraient divergé ; je constate seulement qu'ils sont conformes **maintenant**.
3. Lave, échelles, vol, toile d'araignée, eau courante, sortie de l'eau, nage verticale pure.
4. Les bruits `float` résiduels (≤ 1,4e-6 ici) ne sont pas traités.
5. Un run par côté et par configuration : aucune répétition dans cette phase (la répétabilité exacte a été montrée en
   E.3 à E.6 sur d'autres scénarios, pas rejouée ici).

## 4. Tests

- `umicraft-client --lib` (`e1::`, `args::`) : 35 ok ; l'égalité des quatre scénarios avec leurs fichiers est testée.
- `umicraft-game` : non modifié (1 397 ok en E.6) ; `physics.rs` inchangé dans cette phase.
- Mesures réelles : 6 paires vanilla / Umicraft (§2).

## 5. Anomalies rencontrées (aucune n'est comptée dans le §2)

1. **Umicraft expulsé par le serveur ("Sent too many packets")** lors de ma première tentative. Le joueur (même nom
   que les runs précédents) rejoignait à sa dernière position, **dans le bassin** (y = 2, z = 12,6), puis était
   téléporté à (0,5 ; 4 ; 0,5). Son journal d'état reste alors figé à y = 4, `onGround` faux, `my = -0,0784`, pour
   7 003 trames, jusqu'à l'expulsion. **Cause non établie.** C'est peut-être un défaut réel d'Umicraft (boucle de
   resynchronisation après une téléportation depuis l'eau) ; il n'a pas été étudié. Je l'ai évité avec un joueur neuf
   par configuration. Traces : `Desktop\umicraft-e2\failed_first_attempt\`.
2. **Départ vanilla à y = 3,0 au lieu de 4,0** dans deux configurations (premier essai de `e3` Depth Strider I et de
   `e4` bottes 0, alors qu'Umicraft démarrait à y = 4). Paires non comparables, écartées ; rejouées avec de nouveaux
   joueurs, elles démarraient à y = 4 des deux côtés. Cause non établie (course entre la téléportation et le
   chargement des colonnes côté client vanilla ?). Traces : `failed_second_attempt\`.
3. **La profondeur du bassin a changé entre configurations** : les configurations `e3` Depth Strider I et II ont été
   jouées après l'approfondissement (y = 1 est de l'eau), donc avec un fond à y = 1,0 au lieu de 2,0. Les deux clients
   jouent toujours dans le même monde, donc la comparaison est valide ; la profondeur n'est pas une variable ici.

## 6. Décision

**Phase E.7 : mesures terminées, aucune divergence de gameplay nouvelle.** Suites possibles : (a) étudier l'anomalie 1
du §5, qui pourrait être un vrai défaut d'Umicraft ; (b) mesurer lave, échelles, vol ; (c) traiter le bruit `float`.
