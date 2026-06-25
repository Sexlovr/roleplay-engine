//! Create / edit a character. A single form drives both: pass `edit_id=None`
//! to create, or `edit_id=Some(id)` to load an existing character and PUT the
//! changes. The form is grouped into labelled sections (Identity, Persona,
//! Greeting, Lorebook, Advanced) so it reads like a real character editor
//! rather than one long stack of inputs. A "card version" dropdown (V1/V2/V3)
//! progressively reveals the richer Tavern fields, and an Import panel ingests
//! a SillyTavern card from pasted JSON or a `.png`/`.json` file.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;

use shared::dto::{NewCharacterReq, UpdateCharacterReq};
use shared::types::{Character, LoreEntry};

use crate::api;
use crate::Page;

/// One lorebook entry, modelled as per-field signals so each input edits in
/// place without re-rendering the whole list (keyed by `id` in a `<For>`, so
/// typing never steals focus). `keys` is the comma-separated UI string.
#[derive(Clone, Copy)]
struct LoreRow {
    id: usize,
    keys: RwSignal<String>,
    content: RwSignal<String>,
    enabled: RwSignal<bool>,
}

/// All editable fields, held as signals so create + edit share one form body.
#[derive(Clone, Copy)]
struct Form {
    name: RwSignal<String>,
    tagline: RwSignal<String>,
    avatar: RwSignal<String>,
    creator: RwSignal<String>,
    tags: RwSignal<String>, // comma-separated in the UI
    nsfw: RwSignal<bool>,
    personality: RwSignal<String>,
    description: RwSignal<String>,
    scenario: RwSignal<String>,
    first_message: RwSignal<String>,
    // V2/V3
    mes_example: RwSignal<String>,
    system_prompt: RwSignal<String>,
    post_history_instructions: RwSignal<String>,
    alternate_greetings: RwSignal<String>, // one per line
    creator_notes: RwSignal<String>,
    spec_version: RwSignal<String>,
    // Lorebook / world-info entries + a monotonic id source for stable keys.
    lore: RwSignal<Vec<LoreRow>>,
    next_lore_id: RwSignal<usize>,
}

impl Form {
    fn new() -> Self {
        Form {
            name: RwSignal::new(String::new()),
            tagline: RwSignal::new(String::new()),
            avatar: RwSignal::new(String::new()),
            creator: RwSignal::new(String::new()),
            tags: RwSignal::new(String::new()),
            nsfw: RwSignal::new(false),
            personality: RwSignal::new(String::new()),
            description: RwSignal::new(String::new()),
            scenario: RwSignal::new(String::new()),
            first_message: RwSignal::new(String::new()),
            mes_example: RwSignal::new(String::new()),
            system_prompt: RwSignal::new(String::new()),
            post_history_instructions: RwSignal::new(String::new()),
            alternate_greetings: RwSignal::new(String::new()),
            creator_notes: RwSignal::new(String::new()),
            spec_version: RwSignal::new(String::new()),
            lore: RwSignal::new(Vec::new()),
            next_lore_id: RwSignal::new(0),
        }
    }

    fn load(&self, c: &Character) {
        self.name.set(c.name.clone());
        self.tagline.set(c.tagline.clone());
        self.avatar.set(c.avatar.clone());
        self.creator.set(c.creator.clone());
        self.tags.set(c.tags.join(", "));
        self.nsfw.set(c.nsfw);
        self.personality.set(c.personality.clone());
        self.description.set(c.description.clone());
        self.scenario.set(c.scenario.clone());
        self.first_message.set(c.first_message.clone());
        self.mes_example.set(c.mes_example.clone());
        self.system_prompt.set(c.system_prompt.clone());
        self.post_history_instructions.set(c.post_history_instructions.clone());
        self.alternate_greetings.set(c.alternate_greetings.join("\n"));
        self.creator_notes.set(c.creator_notes.clone());
        self.spec_version.set(c.spec_version.clone());
        self.set_lore(&c.lorebook);
    }

    fn tags_vec(&self) -> Vec<String> {
        self.tags
            .get_untracked()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn greetings_vec(&self) -> Vec<String> {
        self.alternate_greetings
            .get_untracked()
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Replace the working lore list from stored entries, reseeding stable ids.
    fn set_lore(&self, entries: &[LoreEntry]) {
        let rows: Vec<LoreRow> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| LoreRow {
                id: i,
                keys: RwSignal::new(e.keys.join(", ")),
                content: RwSignal::new(e.content.clone()),
                enabled: RwSignal::new(e.enabled),
            })
            .collect();
        self.next_lore_id.set(rows.len());
        self.lore.set(rows);
    }

    /// Collapse the working rows into storable entries, dropping fully-empty
    /// ones (no keys and no content).
    fn lore_vec(&self) -> Vec<LoreEntry> {
        self.lore
            .get_untracked()
            .into_iter()
            .filter_map(|r| {
                let keys: Vec<String> = r
                    .keys
                    .get_untracked()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let content = r.content.get_untracked().trim().to_string();
                if keys.is_empty() && content.is_empty() {
                    return None;
                }
                Some(LoreEntry { keys, content, enabled: r.enabled.get_untracked() })
            })
            .collect()
    }

    fn to_new(&self) -> NewCharacterReq {
        NewCharacterReq {
            name: self.name.get_untracked().trim().to_string(),
            tagline: Some(self.tagline.get_untracked()),
            description: Some(self.description.get_untracked()),
            personality: Some(self.personality.get_untracked()),
            scenario: Some(self.scenario.get_untracked()),
            first_message: Some(self.first_message.get_untracked()),
            avatar: Some(self.avatar.get_untracked()),
            tags: Some(self.tags_vec()),
            creator: Some(self.creator.get_untracked()),
            nsfw: Some(self.nsfw.get_untracked()),
            spec_version: Some(self.spec_version.get_untracked()),
            creator_notes: Some(self.creator_notes.get_untracked()),
            system_prompt: Some(self.system_prompt.get_untracked()),
            post_history_instructions: Some(self.post_history_instructions.get_untracked()),
            mes_example: Some(self.mes_example.get_untracked()),
            alternate_greetings: Some(self.greetings_vec()),
            lorebook: Some(self.lore_vec()),
        }
    }

    fn to_update(&self) -> UpdateCharacterReq {
        UpdateCharacterReq {
            name: Some(self.name.get_untracked().trim().to_string()),
            tagline: Some(self.tagline.get_untracked()),
            description: Some(self.description.get_untracked()),
            personality: Some(self.personality.get_untracked()),
            scenario: Some(self.scenario.get_untracked()),
            first_message: Some(self.first_message.get_untracked()),
            avatar: Some(self.avatar.get_untracked()),
            tags: Some(self.tags_vec()),
            creator: Some(self.creator.get_untracked()),
            nsfw: Some(self.nsfw.get_untracked()),
            spec_version: Some(self.spec_version.get_untracked()),
            creator_notes: Some(self.creator_notes.get_untracked()),
            system_prompt: Some(self.system_prompt.get_untracked()),
            post_history_instructions: Some(self.post_history_instructions.get_untracked()),
            mes_example: Some(self.mes_example.get_untracked()),
            alternate_greetings: Some(self.greetings_vec()),
            lorebook: Some(self.lore_vec()),
        }
    }
}

#[component]
pub fn Create(edit_id: Option<i64>) -> impl IntoView {
    let page = use_context::<RwSignal<Page>>().unwrap();
    let form = Form::new();
    // Card version controls which advanced fields are shown: v1 = none,
    // v2/v3 = the full set. Stored as "", "2.0", "3.0" on the character.
    let version = RwSignal::new("2.0".to_string());
    let saving = RwSignal::new(false);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let show_import = RwSignal::new(false);
    let import_text = RwSignal::new(String::new());
    let loaded = RwSignal::new(edit_id.is_none());
    // Set when an edit-load fails, so we never PUT a blank form over the
    // existing character (every field is wrapped in Some by to_update()).
    let load_failed = RwSignal::new(false);

    // When editing, fetch the existing character and seed the form.
    if let Some(id) = edit_id {
        spawn_local(async move {
            if let Ok(c) = api::get_character(id).await {
                form.load(&c);
                let v = c.spec_version.clone();
                version.set(if v.is_empty() { "1.0".into() } else { v });
                loaded.set(true);
            } else {
                error.set(Some("Could not load this character.".into()));
                load_failed.set(true);
                loaded.set(true);
            }
        });
    }

    let is_advanced = move || version.get() != "1.0";

    // --- lorebook editing ---
    let add_lore = move |_| {
        let id = form.next_lore_id.get_untracked();
        form.next_lore_id.set(id + 1);
        form.lore.update(|v| {
            v.push(LoreRow {
                id,
                keys: RwSignal::new(String::new()),
                content: RwSignal::new(String::new()),
                enabled: RwSignal::new(true),
            })
        });
    };
    let remove_lore = move |row_id: usize| {
        form.lore.update(|v| v.retain(|r| r.id != row_id));
    };

    // --- avatar upload ---
    // No size limit: the image is decoded and downscaled client-side to a small
    // JPEG before storing, so any-size photo is accepted.
    let on_avatar_file = move |ev: web_sys::Event| {
        let input: HtmlInputElement = ev.target().unwrap().unchecked_into();
        let Some(files) = input.files() else { return };
        let Some(file) = files.get(0) else { return };
        crate::upload::read_image_scaled(file, 768, move |res| match res {
            Ok(data) => form.avatar.set(data),
            Err(e) => error.set(Some(format!("Image read failed: {e}"))),
        });
    };

    // --- import: apply parsed card JSON into the form ---
    let apply_card = move |json: String, avatar: Option<String>| {
        match shared::card::parse_card(&json, avatar) {
            Ok(req) => {
                form.name.set(req.name);
                if let Some(v) = req.tagline { form.tagline.set(v); }
                if let Some(v) = req.description { form.description.set(v); }
                if let Some(v) = req.personality { form.personality.set(v); }
                if let Some(v) = req.scenario { form.scenario.set(v); }
                if let Some(v) = req.first_message { form.first_message.set(v); }
                if let Some(v) = req.avatar { form.avatar.set(v); }
                if let Some(v) = req.tags { form.tags.set(v.join(", ")); }
                if let Some(v) = req.creator { form.creator.set(v); }
                if let Some(v) = req.mes_example { form.mes_example.set(v); }
                if let Some(v) = req.system_prompt { form.system_prompt.set(v); }
                if let Some(v) = req.post_history_instructions { form.post_history_instructions.set(v); }
                if let Some(v) = req.alternate_greetings { form.alternate_greetings.set(v.join("\n")); }
                if let Some(v) = req.creator_notes { form.creator_notes.set(v); }
                if let Some(v) = req.lorebook { form.set_lore(&v); }
                let v = req.spec_version.unwrap_or_default();
                version.set(if v.is_empty() { "2.0".into() } else { v });
                show_import.set(false);
                import_text.set(String::new());
                error.set(None);
            }
            Err(e) => error.set(Some(format!("Import failed: {e}"))),
        }
    };

    let on_import_json = move |_| {
        let txt = import_text.get_untracked();
        if txt.trim().is_empty() {
            error.set(Some("Paste a character-card JSON first.".into()));
            return;
        }
        apply_card(txt, None);
    };

    let on_import_file = move |ev: web_sys::Event| {
        let input: HtmlInputElement = ev.target().unwrap().unchecked_into();
        let Some(files) = input.files() else { return };
        let Some(file) = files.get(0) else { return };
        let name = file.name().to_lowercase();
        if name.ends_with(".json") {
            // Read the raw bytes and interpret as UTF-8 JSON text.
            crate::upload::read_as_bytes(file, move |res| match res {
                Ok(bytes) => apply_card(String::from_utf8_lossy(&bytes).into_owned(), None),
                Err(e) => error.set(Some(format!("File read failed: {e}"))),
            });
        } else {
            // Treat as a PNG character card: read bytes, extract embedded JSON,
            // and use the image itself as the avatar.
            crate::upload::read_as_bytes(file, move |res| match res {
                Ok(bytes) => match shared::card::extract_png_card(&bytes) {
                    Some(json) => {
                        // Use the card art itself as the avatar (deliberate import).
                        let avatar =
                            Some(format!("data:image/png;base64,{}", shared::card::base64_encode(&bytes)));
                        apply_card(json, avatar);
                    }
                    None => error.set(Some("That PNG has no embedded character card.".into())),
                },
                Err(e) => error.set(Some(format!("File read failed: {e}"))),
            });
        }
    };

    // --- save ---
    let do_save = move |_| {
        if saving.get_untracked() {
            return;
        }
        // A failed edit-load left the form blank; refuse to overwrite the stored
        // character with empties.
        if load_failed.get_untracked() {
            return;
        }
        if form.name.get_untracked().trim().is_empty() {
            error.set(Some("Please give your character a name.".into()));
            return;
        }
        // Persist the chosen version ("1.0" → stored as "" for a plain V1 card).
        let v = version.get_untracked();
        form.spec_version.set(if v == "1.0" { String::new() } else { v });
        saving.set(true);
        error.set(None);
        spawn_local(async move {
            let result = match edit_id {
                Some(id) => api::update_character(id, &form.to_update()).await.map(|c| c.id),
                None => api::create_character(&form.to_new()).await.map(|c| c.id),
            };
            saving.set(false);
            match result {
                Ok(id) => page.set(Page::Character(id)),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let heading = if edit_id.is_some() { "Edit Character" } else { "Create a Character" };

    view! {
        <div class="create">
            <button class="charpage__back" on:click=move |_| page.set(Page::Home)>
                "\u{2190} Back"
            </button>

            {move || (!loaded.get()).then(|| view! { <p class="hero__sub">"Loading\u{2026}"</p> })}

            {move || loaded.get().then(move || view! {
            <div class="create__card">
                <div class="create__hdr">
                    <h1 class="create__title">{heading}</h1>
                    <div class="create__hdractions">
                        <button class="btn create__import-btn"
                            on:click=move |_| show_import.update(|v| *v = !*v)>
                            "\u{2B07} Import card"
                        </button>
                        <select class="field create__version"
                            on:change=move |ev| version.set(event_target_value(&ev))
                            prop:value=move || version.get()>
                            <option value="1.0">"V1 — basic"</option>
                            <option value="2.0">"V2 — full"</option>
                            <option value="3.0">"V3 — full"</option>
                        </select>
                    </div>
                </div>

                {move || show_import.get().then(|| view! {
                    <div class="import-panel">
                        <div class="field-hint">
                            "Paste a SillyTavern / character-card JSON, or upload a "
                            "card file (.png with embedded data, or .json). V1, V2 and V3 are supported."
                        </div>
                        <textarea class="field field--code" rows="5"
                            placeholder="{ \"spec\": \"chara_card_v2\", \"data\": { ... } }"
                            prop:value=move || import_text.get()
                            on:input=move |ev| import_text.set(event_target_value(&ev)) ></textarea>
                        <div class="import-panel__actions">
                            <label class="btn create__filebtn">
                                "\u{1F4C1} Choose file"
                                <input type="file" accept=".png,.json,image/png,application/json"
                                    class="avatar-file" on:change=on_import_file />
                            </label>
                            <button class="btn btn--login" on:click=on_import_json>"Parse JSON"</button>
                        </div>
                    </div>
                })}

                {move || error.get().map(|e| view! { <div class="settings-error">{e}</div> })}

                // --- Section: Identity ---
                <section class="create__section">
                    <div class="create__secthdr">
                        <h2 class="create__secttitle">"Identity"</h2>
                        <span class="create__sectsub">"How the character shows up in the gallery."</span>
                    </div>
                    <div class="create__row">
                        <div class="create__avatar">
                            <img class="create__avatarimg"
                                src=move || {
                                    let a = form.avatar.get();
                                    if a.is_empty() { "https://picsum.photos/seed/new/400/533".to_string() } else { a }
                                }
                                alt="avatar preview" />
                            <label class="avatar-upload">
                                "\u{1F4F7} Upload"
                                <input type="file" accept="image/*" class="avatar-file" on:change=on_avatar_file />
                            </label>
                        </div>

                        <div class="create__fields">
                            <label class="settings-row">
                                <span>"Name *"</span>
                                <input class="field" placeholder="e.g. Seraphina"
                                    prop:value=move || form.name.get()
                                    on:input=move |ev| form.name.set(event_target_value(&ev)) />
                            </label>
                            <label class="settings-row">
                                <span>"Tagline"<small>" — one-line hook on the card"</small></span>
                                <input class="field" placeholder="A wandering knight with a secret."
                                    prop:value=move || form.tagline.get()
                                    on:input=move |ev| form.tagline.set(event_target_value(&ev)) />
                            </label>
                            <label class="settings-row">
                                <span>"Avatar URL"<small>" — or use Upload"</small></span>
                                <input class="field" placeholder="https://..."
                                    prop:value=move || form.avatar.get()
                                    on:input=move |ev| form.avatar.set(event_target_value(&ev)) />
                            </label>
                            <div class="create__inline">
                                <label class="settings-row">
                                    <span>"Creator"</span>
                                    <input class="field" placeholder="you"
                                        prop:value=move || form.creator.get()
                                        on:input=move |ev| form.creator.set(event_target_value(&ev)) />
                                </label>
                                <label class="settings-row settings-row--check">
                                    <input type="checkbox" prop:checked=move || form.nsfw.get()
                                        on:change=move |ev| form.nsfw.set(event_target_checked(&ev)) />
                                    <span>"NSFW"</span>
                                </label>
                            </div>
                            <label class="settings-row">
                                <span>"Tags"<small>" — comma separated"</small></span>
                                <input class="field" placeholder="fantasy, knight, romance"
                                    prop:value=move || form.tags.get()
                                    on:input=move |ev| form.tags.set(event_target_value(&ev)) />
                            </label>
                        </div>
                    </div>
                </section>

                // --- Section: Persona ---
                <section class="create__section">
                    <div class="create__secthdr">
                        <h2 class="create__secttitle">"Persona"</h2>
                        <span class="create__sectsub">"Who they are — sent to the model as context."</span>
                    </div>
                    <label class="settings-row">
                        <span>"Personality"<small>" — core traits & voice"</small></span>
                        <textarea class="field field--code" rows="4"
                            prop:value=move || form.personality.get()
                            on:input=move |ev| form.personality.set(event_target_value(&ev)) ></textarea>
                    </label>
                    <label class="settings-row">
                        <span>"Description"<small>" — background, appearance, world"</small></span>
                        <textarea class="field field--code" rows="5"
                            prop:value=move || form.description.get()
                            on:input=move |ev| form.description.set(event_target_value(&ev)) ></textarea>
                    </label>
                    <label class="settings-row">
                        <span>"Scenario"<small>" — the situation the chat opens in"</small></span>
                        <textarea class="field field--code" rows="3"
                            prop:value=move || form.scenario.get()
                            on:input=move |ev| form.scenario.set(event_target_value(&ev)) ></textarea>
                    </label>
                </section>

                // --- Section: Greeting ---
                <section class="create__section">
                    <div class="create__secthdr">
                        <h2 class="create__secttitle">"Greeting"</h2>
                        <span class="create__sectsub">"The first thing they say when a chat begins."</span>
                    </div>
                    <label class="settings-row">
                        <span>"First message"<small>" — the character's opening line"</small></span>
                        <textarea class="field field--code" rows="4"
                            prop:value=move || form.first_message.get()
                            on:input=move |ev| form.first_message.set(event_target_value(&ev)) ></textarea>
                    </label>
                    {move || is_advanced().then(|| view! {
                        <label class="settings-row">
                            <span>"Alternate greetings"<small>" — one per line; swipe between them on a new chat"</small></span>
                            <textarea class="field field--code" rows="3"
                                prop:value=move || form.alternate_greetings.get()
                                on:input=move |ev| form.alternate_greetings.set(event_target_value(&ev)) ></textarea>
                        </label>
                    })}
                </section>

                // --- Section: Lorebook ---
                <section class="create__section">
                    <div class="create__secthdr">
                        <h2 class="create__secttitle">"\u{1F4D6} Lorebook"</h2>
                        <span class="create__sectsub">
                            "World info injected when a keyword is mentioned. Leave keys blank for an always-on entry."
                        </span>
                    </div>
                    <div class="lore">
                        {move || {
                            let empty = form.lore.get().is_empty();
                            empty.then(|| view! {
                                <div class="lore__empty">"No entries yet — add lore the model should know about your world."</div>
                            })
                        }}
                        <For
                            each=move || form.lore.get()
                            key=|r| r.id
                            children=move |r: LoreRow| {
                                view! {
                                    <div class="lore__entry" class=("lore__entry--off", move || !r.enabled.get())>
                                        <div class="lore__top">
                                            <input class="field lore__keys" placeholder="keys, comma, separated"
                                                prop:value=move || r.keys.get()
                                                on:input=move |ev| r.keys.set(event_target_value(&ev)) />
                                            <label class="lore__toggle" title="Enable / disable this entry">
                                                <input type="checkbox" prop:checked=move || r.enabled.get()
                                                    on:change=move |ev| r.enabled.set(event_target_checked(&ev)) />
                                                <span>"On"</span>
                                            </label>
                                            <button class="lore__del" title="Remove entry"
                                                on:click=move |_| remove_lore(r.id)>"\u{1F5D1}"</button>
                                        </div>
                                        <textarea class="field field--code lore__content" rows="3"
                                            placeholder="What the model should know when these keys come up..."
                                            prop:value=move || r.content.get()
                                            on:input=move |ev| r.content.set(event_target_value(&ev)) ></textarea>
                                    </div>
                                }
                            }
                        />
                        <button class="btn lore__add" on:click=add_lore>"\u{2795} Add lore entry"</button>
                    </div>
                </section>

                // --- Section: Advanced (V2/V3) ---
                {move || is_advanced().then(|| view! {
                    <section class="create__section">
                        <div class="create__secthdr">
                            <h2 class="create__secttitle">"Advanced"</h2>
                            <span class="create__sectsub">"V2 / V3 card fields for fine-grained control."</span>
                        </div>
                        <label class="settings-row">
                            <span>"Example dialogue"<small>" — shows the model how they talk"</small></span>
                            <textarea class="field field--code" rows="4"
                                prop:value=move || form.mes_example.get()
                                on:input=move |ev| form.mes_example.set(event_target_value(&ev)) ></textarea>
                        </label>
                        <label class="settings-row">
                            <span>"System prompt"<small>" — character-level instructions to the model"</small></span>
                            <textarea class="field field--code" rows="3"
                                prop:value=move || form.system_prompt.get()
                                on:input=move |ev| form.system_prompt.set(event_target_value(&ev)) ></textarea>
                        </label>
                        <label class="settings-row">
                            <span>"Post-history instructions"<small>" — reinforced after the chat (UJB)"</small></span>
                            <textarea class="field field--code" rows="3"
                                prop:value=move || form.post_history_instructions.get()
                                on:input=move |ev| form.post_history_instructions.set(event_target_value(&ev)) ></textarea>
                        </label>
                        <label class="settings-row">
                            <span>"Creator notes"<small>" — not sent to the model"</small></span>
                            <textarea class="field field--code" rows="2"
                                prop:value=move || form.creator_notes.get()
                                on:input=move |ev| form.creator_notes.set(event_target_value(&ev)) ></textarea>
                        </label>
                    </section>
                })}

                <div class="create__actions">
                    <button class="btn" on:click=move |_| page.set(Page::Home)>"Cancel"</button>
                    <button class="btn btn--login" prop:disabled=move || saving.get() || load_failed.get() on:click=do_save>
                        {move || if saving.get() {
                            "Saving\u{2026}".to_string()
                        } else if edit_id.is_some() {
                            "Save changes".to_string()
                        } else {
                            "Create character".to_string()
                        }}
                    </button>
                </div>
            </div>
            })}
        </div>
    }
}
