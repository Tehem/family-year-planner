// Interface languages (SPEC §19.2): French (default) and English.
//
// The language is per browser: the `annee_lang` cookie, set by the language switch (and by
// `?lang=` on the server). Static HTML keeps its French text and is translated in place from
// `data-i18n` attributes, so a French page never depends on this module to be readable.

export type Lang = "fr" | "en";
export const LANG_COOKIE = "annee_lang";

const FR = {
  "app.title": "Année en famille",
  "lang.label": "Langue",
  "year.prev": "Année précédente",
  "year.next": "Année suivante",
  "view.label": "Affichage",
  "view.week": "Semaines",
  "view.month": "Mois",
  "legend.label": "Catégories affichées",
  "legend.static": "Catégories",
  "who.label": "Événements de",
  "who.all": "Tous",
  "both.name": "Les deux",
  "both.title": "Vacances scolaires et crèche ou centre de loisirs fermé le même jour",
  "btn.add": "Ajouter un événement",
  "btn.print": "Imprimer",
  "btn.settings": "Réglages",
  "btn.logout": "Se déconnecter",
  "btn.reset": "Réinitialiser la démo",
  "btn.reset.title": "Remet les événements d’exemple de la démonstration.",
  "btn.cancel": "Annuler",
  "btn.retry": "Réessayer",
  "btn.copy": "Copier",
  "btn.copied": "Copié",
  "btn.copyFailed": "Copie impossible",
  "print.aria": "Impression",
  "print.format": "Format",
  "print.a3": "A3 paysage",
  "print.a4": "A4 paysage",
  "print.without": "sans {list}",
  "print.filters": "Filtres : {list}",
  "print.printedOn": "Imprimé le {date}",
  "banner.error": "Impossible de charger l’agenda.",
  "hint.rotate": "Tournez votre téléphone pour voir l’année entière.",
  "panel.aria": "Détails",
  "panel.close": "Fermer",
  "dlg.new": "Nouvel événement",
  "dlg.edit": "Modifier l’événement",
  "f.title": "Titre",
  "f.title.ph": "ex. Week-end à la maison de la plage",
  "f.cat": "Catégorie",
  "f.start": "Premier jour",
  "f.end": "Dernier jour",
  "f.birth": "Année de naissance",
  "f.birth.ph": "facultatif",
  "f.who": "Qui est concerné ?",
  "f.loc": "Lieu",
  "f.notes": "Notes",
  "dlg.pickCat": "Choisir une catégorie",
  "dlg.add": "Ajouter",
  "dlg.save": "Enregistrer",
  "dlg.saving": "Enregistrement…",
  "err.title": "Donnez un titre à l’événement.",
  "err.cat": "Choisissez une catégorie.",
  "err.dates": "Choisissez un premier et un dernier jour.",
  "err.order": "Le dernier jour ne peut pas être avant le premier.",
  "err.person": "Choisissez au moins une personne.",
  "err.birth": "Année de naissance invalide.",
  "err.generic": "Une erreur est survenue. Réessayez dans un instant.",
  "err.short": "Une erreur est survenue.",
  "err.network": "Impossible de joindre le serveur. Réessayez dans un instant.",
  "err.session": "Session expirée.",
  "day.nothing": "Rien de prévu ce jour-là.",
  "day.add": "Ajouter un événement ce jour-là",
  "tip.nothing": "Rien de prévu",
  "age.one": "{n} an",
  "age.many": "{n} ans",
  "time.range": "De {a} à {b}",
  "time.at": "À {a}",
  "meta.cat": "Catégorie",
  "meta.duration": "Durée",
  "meta.who": "Qui",
  "meta.loc": "Lieu",
  "meta.notes": "Notes",
  "meta.addedBy": "Ajouté par",
  "meta.modifiedBy": "Modifié par",
  "meta.day": "{n} jour",
  "meta.days": "{n} jours",
  "meta.everyone": "Toute la famille",
  "link.gcal": "Ouvrir dans Google Agenda",
  "ev.imported": "Importé automatiquement",
  "ev.edit": "Modifier",
  "ev.delete": "Supprimer",
  "confirm.aria": "Confirmer la suppression",
  "confirm.q": "Supprimer « {title} » ?",
  "confirm.series": "Cet événement se répète chaque année : toutes les occurrences seront supprimées.",
  "range.multi": "du {a} au {b} {year}",
  "kiosk.offline": "Hors ligne depuis {time}",
  "fmt.at": "à",
  "fmt.never": "Jamais",
  // Réglages
  "set.title": "Réglages",
  "set.pageTitle": "Réglages · Année en famille",
  "set.back": "← Retour à l’année",
  "set.agents": "Accès des agents",
  "set.tokens": "Jetons personnels",
  "set.tokens.table": "Jetons personnels",
  "set.tokens.name": "Nom",
  "set.tokens.rights": "Droits",
  "set.tokens.created": "Créé le",
  "set.tokens.lastUsed": "Dernière utilisation",
  "set.tokens.empty": "Aucun jeton pour l’instant.",
  "set.tokens.ph": "ex. Grok API",
  "set.tokens.read": "Lecture seule",
  "set.tokens.readWrite": "Lecture et écriture",
  "set.tokens.create": "Créer le jeton",
  "set.tokens.nameRequired": "Donnez un nom au jeton.",
  "set.tokens.once": "Copiez ce jeton maintenant : il ne sera plus affiché.",
  "set.tokens.secretLabel": "Jeton",
  "set.kioskBadge": "Kiosque",
  "set.actions": "Actions",
  "set.revoke": "Révoquer",
  "set.revokeAria": "Révoquer {name}",
  "set.revokeQ": "Révoquer l’accès de « {name} » ?",
  "set.revokeNote": "L’accès cesse immédiatement.",
  "set.grants": "Connexions OAuth",
  "set.grants.app": "Application",
  "set.grants.redirect": "Redirection",
  "set.grants.authorized": "Autorisé le",
  "set.grants.empty": "Aucune application connectée.",
  "set.mcp": "Adresse du serveur MCP",
  "set.newSecret": "Nouveau secret",
  "set.kiosk": "Tablette (mode kiosque)",
  "set.kiosk.help": "Un lien kiosque ouvre l’année en lecture seule sur une tablette, sans connexion.",
  "set.kiosk.create": "Créer un lien kiosque",
  "set.kiosk.once": "Copiez ce lien maintenant : il ne sera plus affiché.",
  "set.kiosk.label": "Lien kiosque",
  "set.imports": "Imports",
  "set.imports.now": "Mettre à jour maintenant",
  "set.imports.running": "Import en cours…",
  "set.imports.last": "Dernier import : {date}.",
  "set.imports.none": "Aucun import pour l’instant.",
  "set.imports.table": "Derniers imports",
  "set.imports.date": "Date",
  "set.imports.result": "Résultat",
  "set.imports.created": "Créés",
  "set.imports.updated": "Mis à jour",
  "set.imports.deleted": "Supprimés",
  "set.imports.unchanged": "Inchangés",
  "set.imports.error": "Erreur",
  "set.imports.success": "Réussi",
  "set.imports.failed": "Échec",
  "set.imports.runningStatus": "En cours",
  // Catégories (editor)
  "types.title": "Catégories",
  "types.help":
    "Renommez les catégories et choisissez leur couleur : la couleur correspond à une couleur d’événement de Google Agenda. Vous pouvez aussi ajouter vos propres catégories.",
  "types.fr": "Français",
  "types.en": "English",
  "types.singular": "Nom",
  "types.plural": "Nom au pluriel (légende)",
  "types.color": "Couleur dans Google Agenda",
  "types.custom": "Catégorie ajoutée",
  "types.builtin": "Catégorie de base",
  "types.noColor": "Aucune couleur (événements sans couleur)",
  "types.delete": "Supprimer la catégorie",
  "types.add": "Ajouter une catégorie",
  "types.add.free": "{n} couleur(s) libre(s) dans Google Agenda.",
  "types.add.max": "Trois catégories ajoutées au maximum : Google Agenda n’offre que 11 couleurs.",
  "types.add.none": "Plus aucune couleur libre dans Google Agenda.",
  "types.add.saveFirst": "Enregistrez d’abord la suppression pour pouvoir ajouter une nouvelle catégorie.",
  "types.newName": "Nouvelle catégorie",
  "types.save": "Enregistrer les catégories",
  "types.reset": "Annuler les modifications",
  "types.saved": "Catégories enregistrées.",
  "types.dup": "Cette couleur est déjà utilisée par « {name} ».",
  "types.invalid": "Corrigez les champs signalés avant d’enregistrer.",
  "types.nameRequired": "Renseignez les quatre noms.",
  "types.confirm.title": "Modifier les couleurs dans Google Agenda ?",
  "types.confirm.changes": "{n} événement(s) seront recolorés dans Google Agenda.",
  "types.confirm.change": "« {name} » : {n} événement(s), de {from} à {to}.",
  "types.confirm.delete": "« {name} » sera supprimée : {n} événement(s) deviendront « Non classés ».",
  "types.confirm.orphans": "{n} événement(s) existants de cette couleur deviendront des événements de la catégorie choisie.",
  "types.confirm.ok": "Appliquer",
  "types.confirm.wait": "Cela peut prendre quelques instants.",
  "types.migration": "Une modification des couleurs n’est pas terminée. Les modifications sont suspendues tant qu’elle n’est pas reprise.",
  "types.migration.resume": "Reprendre",
  "types.migration.error": "Dernière erreur : {error}",
  "types.none": "(aucune)",
  // Login
  "login.pageTitle": "Connexion · Année en famille",
  "login.lead": "Connectez-vous pour voir l’année de la famille.",
  "login.google": "Se connecter avec Google",
  "login.demo": "Entrer en mode démo",
  "login.notAllowed": "Ce compte Google n’est pas autorisé.",
  "login.invalid": "Votre demande de connexion a expiré. Réessayez.",
  "login.denied": "La connexion a été annulée.",
  "login.failed": "La connexion avec Google a échoué. Réessayez.",
  "login.kiosk": "Ce lien kiosque n’est plus valide.",
  "login.generic": "La connexion a échoué. Réessayez.",
} as const;

export type Key = keyof typeof FR;

const EN: Record<Key, string> = {
  "app.title": "Family year",
  "lang.label": "Language",
  "year.prev": "Previous year",
  "year.next": "Next year",
  "view.label": "View",
  "view.week": "Weeks",
  "view.month": "Months",
  "legend.label": "Displayed categories",
  "legend.static": "Categories",
  "who.label": "Events for",
  "who.all": "Everyone",
  "both.name": "Both",
  "both.title": "School holiday and daycare or leisure centre closed on the same day",
  "btn.add": "Add an event",
  "btn.print": "Print",
  "btn.settings": "Settings",
  "btn.logout": "Sign out",
  "btn.reset": "Reset the demo",
  "btn.reset.title": "Restores the demo’s sample events.",
  "btn.cancel": "Cancel",
  "btn.retry": "Retry",
  "btn.copy": "Copy",
  "btn.copied": "Copied",
  "btn.copyFailed": "Copy failed",
  "print.aria": "Printing",
  "print.format": "Format",
  "print.a3": "A3 landscape",
  "print.a4": "A4 landscape",
  "print.without": "without {list}",
  "print.filters": "Filters: {list}",
  "print.printedOn": "Printed on {date}",
  "banner.error": "Unable to load the calendar.",
  "hint.rotate": "Rotate your phone to see the whole year.",
  "panel.aria": "Details",
  "panel.close": "Close",
  "dlg.new": "New event",
  "dlg.edit": "Edit the event",
  "f.title": "Title",
  "f.title.ph": "e.g. Weekend at the beach house",
  "f.cat": "Category",
  "f.start": "First day",
  "f.end": "Last day",
  "f.birth": "Year of birth",
  "f.birth.ph": "optional",
  "f.who": "Who is involved?",
  "f.loc": "Place",
  "f.notes": "Notes",
  "dlg.pickCat": "Choose a category",
  "dlg.add": "Add",
  "dlg.save": "Save",
  "dlg.saving": "Saving…",
  "err.title": "Give the event a title.",
  "err.cat": "Choose a category.",
  "err.dates": "Choose a first and a last day.",
  "err.order": "The last day cannot be before the first day.",
  "err.person": "Choose at least one person.",
  "err.birth": "Invalid birth year.",
  "err.generic": "Something went wrong. Try again in a moment.",
  "err.short": "Something went wrong.",
  "err.network": "Unable to reach the server. Try again in a moment.",
  "err.session": "Session expired.",
  "day.nothing": "Nothing planned that day.",
  "day.add": "Add an event that day",
  "tip.nothing": "Nothing planned",
  "age.one": "age {n}",
  "age.many": "age {n}",
  "time.range": "From {a} to {b}",
  "time.at": "At {a}",
  "meta.cat": "Category",
  "meta.duration": "Duration",
  "meta.who": "Who",
  "meta.loc": "Place",
  "meta.notes": "Notes",
  "meta.addedBy": "Added by",
  "meta.modifiedBy": "Modified by",
  "meta.day": "{n} day",
  "meta.days": "{n} days",
  "meta.everyone": "The whole family",
  "link.gcal": "Open in Google Calendar",
  "ev.imported": "Imported automatically",
  "ev.edit": "Edit",
  "ev.delete": "Delete",
  "confirm.aria": "Confirm deletion",
  "confirm.q": "Delete “{title}”?",
  "confirm.series": "This event repeats every year: all occurrences will be deleted.",
  "range.multi": "{a} – {b} {year}",
  "kiosk.offline": "Offline since {time}",
  "fmt.at": "at",
  "fmt.never": "Never",
  "set.title": "Settings",
  "set.pageTitle": "Settings · Family year",
  "set.back": "← Back to the year",
  "set.agents": "Agent access",
  "set.tokens": "Personal tokens",
  "set.tokens.table": "Personal tokens",
  "set.tokens.name": "Name",
  "set.tokens.rights": "Permissions",
  "set.tokens.created": "Created on",
  "set.tokens.lastUsed": "Last used",
  "set.tokens.empty": "No tokens yet.",
  "set.tokens.ph": "e.g. Grok API",
  "set.tokens.read": "Read only",
  "set.tokens.readWrite": "Read and write",
  "set.tokens.create": "Create the token",
  "set.tokens.nameRequired": "Give the token a name.",
  "set.tokens.once": "Copy this token now: it will not be shown again.",
  "set.tokens.secretLabel": "Token",
  "set.kioskBadge": "Kiosk",
  "set.actions": "Actions",
  "set.revoke": "Revoke",
  "set.revokeAria": "Revoke {name}",
  "set.revokeQ": "Revoke the access of “{name}”?",
  "set.revokeNote": "Access ends immediately.",
  "set.grants": "OAuth connections",
  "set.grants.app": "Application",
  "set.grants.redirect": "Redirect",
  "set.grants.authorized": "Authorized on",
  "set.grants.empty": "No connected applications.",
  "set.mcp": "MCP server address",
  "set.newSecret": "New secret",
  "set.kiosk": "Tablet (kiosk mode)",
  "set.kiosk.help": "A kiosk link opens the year in read-only mode on a tablet, without signing in.",
  "set.kiosk.create": "Create a kiosk link",
  "set.kiosk.once": "Copy this link now: it will not be shown again.",
  "set.kiosk.label": "Kiosk link",
  "set.imports": "Imports",
  "set.imports.now": "Update now",
  "set.imports.running": "Import in progress…",
  "set.imports.last": "Last import: {date}.",
  "set.imports.none": "No import yet.",
  "set.imports.table": "Latest imports",
  "set.imports.date": "Date",
  "set.imports.result": "Result",
  "set.imports.created": "Created",
  "set.imports.updated": "Updated",
  "set.imports.deleted": "Deleted",
  "set.imports.unchanged": "Unchanged",
  "set.imports.error": "Error",
  "set.imports.success": "Succeeded",
  "set.imports.failed": "Failed",
  "set.imports.runningStatus": "Running",
  "types.title": "Categories",
  "types.help":
    "Rename the categories and choose their color: the color matches a Google Calendar event color. You can also add your own categories.",
  "types.fr": "Français",
  "types.en": "English",
  "types.singular": "Name",
  "types.plural": "Plural name (legend)",
  "types.color": "Color in Google Calendar",
  "types.custom": "Added category",
  "types.builtin": "Built-in category",
  "types.noColor": "No color (events without a color)",
  "types.delete": "Delete the category",
  "types.add": "Add a category",
  "types.add.free": "{n} free color(s) left in Google Calendar.",
  "types.add.max": "At most three added categories: Google Calendar only offers 11 colors.",
  "types.add.none": "No free color left in Google Calendar.",
  "types.add.saveFirst": "Save the deletion first to be able to add a new category.",
  "types.newName": "New category",
  "types.save": "Save the categories",
  "types.reset": "Discard changes",
  "types.saved": "Categories saved.",
  "types.dup": "This color is already used by “{name}”.",
  "types.invalid": "Fix the highlighted fields before saving.",
  "types.nameRequired": "Fill in the four names.",
  "types.confirm.title": "Change the colors in Google Calendar?",
  "types.confirm.changes": "{n} event(s) will be recolored in Google Calendar.",
  "types.confirm.change": "“{name}”: {n} event(s), from {from} to {to}.",
  "types.confirm.delete": "“{name}” will be deleted: {n} event(s) will become “Unclassified”.",
  "types.confirm.orphans": "{n} existing event(s) of this color will become events of the chosen category.",
  "types.confirm.ok": "Apply",
  "types.confirm.wait": "This can take a few moments.",
  "types.migration": "A color change is unfinished. Edits are suspended until it is resumed.",
  "types.migration.resume": "Resume",
  "types.migration.error": "Last error: {error}",
  "types.none": "(none)",
  "login.pageTitle": "Sign in · Family year",
  "login.lead": "Sign in to see the family year.",
  "login.google": "Sign in with Google",
  "login.demo": "Enter demo mode",
  "login.notAllowed": "This Google account is not allowed.",
  "login.invalid": "Your sign-in request has expired. Try again.",
  "login.denied": "Sign-in was cancelled.",
  "login.failed": "Signing in with Google failed. Try again.",
  "login.kiosk": "This kiosk link is no longer valid.",
  "login.generic": "Sign-in failed. Try again.",
};

const DICT: Record<Lang, Record<Key, string>> = { fr: FR, en: EN };

function readCookie(): Lang | null {
  const m = document.cookie.split(";").map((c) => c.trim().split("="));
  const v = m.find(([k]) => k === LANG_COOKIE)?.[1];
  return v === "fr" || v === "en" ? v : null;
}

let lang: Lang = readCookie() ?? "fr";

export const getLang = (): Lang => lang;
/** BCP 47 locale used for dates and numbers. */
export const locale = (): string => (lang === "en" ? "en-GB" : "fr-FR");

/** Called with the language the server resolved (cookie, else the instance default). */
export function adoptLang(l: string | undefined): void {
  if ((l === "fr" || l === "en") && l !== lang) {
    lang = l;
    applyStatic();
    if (switchHost) mountLangSwitch(switchHost);
  }
}

/** Pages without a config call (login): the instance default applies only without a cookie. */
export function adoptDefaultLang(l: string | undefined): void {
  if (readCookie() === null) adoptLang(l);
}

let switchHost: HTMLElement | null = null;

export function t(key: Key, params?: Record<string, string | number>): string {
  let s = DICT[lang][key];
  if (params) for (const [k, v] of Object.entries(params)) s = s.replaceAll(`{${k}}`, String(v));
  return s;
}

/** French and English text for the same key (the editor shows both names). */
export const tIn = (l: Lang, key: Key): string => DICT[l][key];

/** Translates the static markup in place:
 *  `data-i18n="key"` (text), `data-i18n-attr="aria-label:key;title:key"` (attributes). */
export function applyStatic(root: ParentNode = document): void {
  document.documentElement.lang = lang;
  root.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
    el.textContent = t(el.dataset["i18n"] as Key);
  });
  root.querySelectorAll<HTMLElement>("[data-i18n-attr]").forEach((el) => {
    for (const pair of (el.dataset["i18nAttr"] ?? "").split(";")) {
      const [attr, key] = pair.split(":");
      if (attr && key) el.setAttribute(attr, t(key as Key));
    }
  });
  const title = document.querySelector<HTMLElement>("title[data-i18n]");
  if (title) document.title = t(title.dataset["i18n"] as Key);
}

export function setLangCookie(l: Lang): void {
  document.cookie = `${LANG_COOKIE}=${l}; Path=/; Max-Age=${365 * 24 * 3600}; SameSite=Lax`;
}

/** Mounts the `FR | EN` switch into `host`; choosing a language reloads the page. */
export function mountLangSwitch(host: HTMLElement): void {
  switchHost = host;
  host.classList.add("seg", "lang");
  host.setAttribute("role", "group");
  host.setAttribute("aria-label", t("lang.label"));
  host.replaceChildren(
    ...(["fr", "en"] as const).map((l) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = l.toUpperCase();
      b.lang = l;
      b.dataset["lang"] = l;
      b.setAttribute("aria-pressed", String(l === lang));
      b.addEventListener("click", () => {
        if (l === lang) return;
        setLangCookie(l);
        // An explicit `?lang=` in the address would win over the new cookie.
        const url = new URL(window.location.href);
        url.searchParams.delete("lang");
        window.location.replace(url);
      });
      return b;
    }),
  );
}
