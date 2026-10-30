# AI Phone

[English](../../README.md) · [Toutes les langues](../../README.md) · Apache-2.0

## Téléchargements et état

Aucun installateur complet du produit n’est disponible. Les aperçus incomplets ont été retirés. Ce dépôt contient du code en développement. La prochaine distribution doit embarquer les images Android/Linux propres, l’émulateur et les services précompilés. L’utilisateur doit pouvoir configurer appareils et modèle dans le client sans rechercher d’image, installer de SDK ni compiler le serveur.

Plateformes visées : macOS Apple Silicon, Windows x64, Linux x64/ARM64 et Android ARM64. macOS et Linux x64 disposent de chaînes expérimentales de développement. L’hôte Windows autonome et la chaîne Linux ARM64 complète restent à terminer. Android exige root et KVM disponible ; un APK ordinaire sans root n’est pas un hôte autonome pris en charge. Aucun paquet autonome complet n’a encore été validé.

## Fonctions et scénarios

AI Phone réunit un véritable Android invité, un bureau Linux léger et des Agents. Par défaut : un téléphone et un ordinateur, chacun avec sa conversation continue. Un Agent peut être partagé ou des modèles distincts peuvent être affectés.

- Utiliser des applications telles que WeChat à partir de l’écran et des notifications réels ; répondre selon les instructions de l’utilisateur. Les restrictions du compte et de l’application restent applicables.
- Recevoir un fichier sur le téléphone, le transférer à Linux, traiter et vérifier un tableau, puis retourner le résultat. Transfert entre appareils et remise à un contact sont deux résultats distincts.
- Ajouter des instructions et interrompre la génération sans perdre messages ni actions. Les notifications peuvent réveiller l’Agent ; l’exploration autonome se met en pause par appareil.
- Partager Skills, documents de connaissance liés et fichiers de travail. Modèles/API, mémoire et connexions externes appartiennent aux Agents. Clash fonctionne dans chaque système invité ; le réseau de l’hôte est direct par défaut.

Les actions sont liées à une observation, un appareil, une tâche, une époque de contrôle et un identifiant unique. Un résultat inconnu est observé avant toute répétition. Le modèle peut mal interpréter une interface ; les journaux ne garantissent pas une précision parfaite.

## Architecture et données

`apps/web` : Next.js App Router, React, Tailwind, shadcn. `apps/client` : Tauri 2. `crates` : Rust/Axum/Tokio. `packages/protocol` : protobuf. PostgreSQL/SQLx conserve l’état ; Redis assure une coordination temporaire. Bun et Cargo servent au développement, Compose aux services. `infra`, `scripts`, `skills`, `config` regroupent exécution et capacités intégrées.

Code et données utilisateur sont séparés. Disques, comptes, conversations, pièces jointes et clés ne sont pas publiés. Les mises à jour doivent employer des artefacts versionnés, sauvegardes, migrations et retours arrière, sans `git pull` directement sur les données utilisateur.

## Suite, responsabilité et licence

Priorités : paquets complets, reprise d’installation, mises à jour signées, stabilité de l’Agent, audio/saisie, vérification des modèles et Skills spécialisés. Viendront ensuite la voix et une téléphonie expérimentale ; appels réels/eSIM ne sont pas implémentés. Le compagnon de bureau a quitté le cœur et reste une idée d’extension.

Interface et première configuration couvrent dix langues. Le sélecteur à gauche de Skills change l’interface ; il ne traduit pas automatiquement les systèmes invités ni les sorties brutes. L’arabe utilise le sens droite à gauche.

L’utilisateur reste responsable des autorisations, comptes, actions externes et conditions des applications. Ne pas employer le système pour tromper, frauder ou agir sans consentement. Code original sous Apache-2.0 ; dépendances sous leurs propres licences. Ces mentions n’effacent pas la responsabilité légale.

[État de publication](release-status.md) · [Architecture](architecture.en.md) · [Compatibilité](compatibility.md) · [Tiers](../../THIRD_PARTY_NOTICES.md) · [Avertissement](../../DISCLAIMER.md)
