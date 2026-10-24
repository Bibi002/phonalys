# Conformité légale

> ⚠️ Ce document est un **guide de travail**, pas un avis juridique. Avant la commercialisation, il doit être validé par un avocat spécialisé en droit de la santé et des données, ainsi que par un consultant réglementaire pour les dispositifs médicaux. À relire au moins une fois par an (dernière révision : octobre 2026).

## 1. Protection des données (RGPD et droit belge)

**Textes :** Règlement (UE) 2016/679 (RGPD) ; loi belge du 30 juillet 2018 relative à la protection des personnes physiques à l'égard des traitements de données à caractère personnel. **Autorité de contrôle :** Autorité de protection des données (APD/GBA).

| Exigence | Article | Réponse de Phonalys |
|---|---|---|
| Base légale des données de santé | Art. 9.2.h | Traitement par un professionnel de santé tenu au secret. Les consentements sont gérés à part (photo, audio, recherche). |
| Minimisation | Art. 5.1.c | Les champs obligatoires sont réduits au strict nécessaire, et les identifiants sont pseudonymisés pour l'IA. |
| Sécurité du traitement | Art. 32 | Chiffrement, 2FA, audit (voir [SECURITE.md](SECURITE.md)). |
| Protection dès la conception et par défaut | Art. 25 | Fonctionnement local par défaut, aucune télémétrie, IA locale uniquement. |
| Registre des activités | Art. 30 | Écran « Conformité » : registre prérempli avec les coordonnées du cabinet, imprimable. |
| Information des personnes | Art. 13 | Écran « Conformité » : notice pour les patients et leurs parents, préremplie, à remettre au premier rendez-vous. |
| AIPD | Art. 35 | Écran « Conformité » : trame d'analyse d'impact avec les risques et les mesures de Phonalys ; le cabinet évalue le risque résiduel (données de santé de mineurs : une AIPD est recommandée). |
| Violation de données | Art. 33–34 | Écran « Conformité » : procédure pas à pas, registre des violations chiffré dans le coffre (art. 33.5) et décompte du délai de **72 h**. La notification elle-même se fait sur le site de l'APD. |
| Droits des personnes | Art. 15–21 | Fiche patient › « Copie du dossier » : export complet en JSON (identité, dossier clinique, documents), après réauthentification et inscrit au journal ; rectification ; effacement complet du dossier, à n'utiliser que dans la limite des obligations de conservation. |
| Sous-traitance | Art. 28 | L'éditeur n'est pas sous-traitant, puisqu'il n'accède à aucune donnée. À formaliser dans le CLUF. |

**Mineurs :** les droits s'exercent par les parents ou représentants légaux. Il faut tenir compte de la capacité de discernement de l'enfant (loi droits du patient, art. 12).

## 2. Droit belge de la santé

- **Loi du 22 août 2002 relative aux droits du patient** (modifiée, notamment, par la loi du 6 février 2024) : droit à un dossier soigneusement tenu, droit de le consulter et d'en obtenir une copie, et droit à l'information. Phonalys exporte le dossier complet (« Copie du dossier ») et imprime chaque bilan ; le journal d'audit garde la trace des consultations et modifications.
- **Loi du 22 avril 2019 relative à la qualité de la pratique des soins de santé** : tenue du dossier patient, dont le contenu minimal est défini, et conservation **au moins 30 ans et au plus 50 ans** après le dernier contact (art. 35). Phonalys n'efface jamais un dossier de lui-même ; la durée figure dans le registre et la notice. Une alerte à l'échéance reste à développer.
- **Profession de logopède** (profession paramédicale) : agrément et visa délivrés par le SPF Santé publique. Le numéro INAMI et le visa figurent sur les documents.
- **Assurance soins de santé (INAMI)** : nomenclature des prestations de logopédie (art. 36) et conditions des bilans pour l'accord du médecin-conseil. Les modèles de bilan Phonalys suivent la structure attendue. **Les seuils et critères restent paramétrables**, parce qu'ils évoluent.
- **Secret professionnel** (Code pénal, art. 458).

## 3. Dispositifs médicaux (MDR)

**Texte :** Règlement (UE) 2017/745, règle 11 de l'annexe VIII.

Un logiciel destiné à fournir des informations utilisées pour prendre des décisions **diagnostiques** est un dispositif médical, au minimum de classe IIa. Cela suppose un organisme notifié, un marquage CE, un système qualité ISO 13485, une gestion des risques ISO 14971 et un cycle de vie logiciel IEC 62304.

**Stratégie proposée (à valider) :**

- **Version 1 :** positionner Phonalys comme un outil de **gestion du dossier, de calcul et d'aide à la rédaction**. Le logopède pose seul l'interprétation clinique, et le logiciel n'émet aucun diagnostic. La terminologie de l'interface et du marketing doit rester cohérente avec cette « destination », qui détermine la qualification.
- **Plus tard :** si des fonctions d'aide au diagnostic sont visées (par exemple une suggestion de profil), engager la démarche de marquage CE. Le code est déjà structuré pour la traçabilité exigée par IEC 62304 (tests, revue, journal des versions).

## 4. Intelligence artificielle (AI Act)

**Texte :** Règlement (UE) 2024/1689, avec des obligations entrées en vigueur progressivement entre 2025 et 2027.

- Un système d'IA qui est un composant de sécurité d'un dispositif médical relevant du MDR est classé **à haut risque**. Rester hors MDR en version 1 maintient l'IA dans la catégorie « assistance à la rédaction ».
- Obligations de **transparence** : signaler clairement les contenus générés par l'IA et garder une trace de leur validation humaine.
- **Maîtrise humaine** : rien n'est envoyé ni signé sans relecture par le praticien.
- Le praticien choisit et héberge son modèle : il est donc **déployeur**, et Phonalys doit lui fournir la documentation d'usage.

## 5. Propriété intellectuelle

- Phonalys **ne reproduit aucun contenu** de batteries existantes (items, images, consignes, normes). Les idées, les domaines évalués et les principes psychométriques ne sont pas protégés ; leur expression l'est.
- Le praticien qui encode une épreuve sous licence doit respecter le contrat de l'éditeur de cette épreuve. Phonalys le lui rappelle sur l'écran de saisie des bilans.
- Nom et logo « Phonalys » : vérifier l'antériorité puis déposer la marque (BOIP pour le Benelux, EUIPO pour l'UE).

## 6. Accessibilité et consommation

- Viser la conformité **WCAG 2.2 AA** (European Accessibility Act, applicable depuis juin 2025 à certains produits et services).
- Pour la vente aux professionnels, prévoir des conditions générales de vente et le CLUF.
