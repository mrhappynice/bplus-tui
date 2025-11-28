// ================================================
// FILE: src/app.rs
// ================================================
use tokio::sync::mpsc;
use crate::api::{self, AppModel, Conversation, Model, ProviderConfig, SearchSource};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum CurrentScreen {
    Launcher,
    Search,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,         // Launcher navigation
    Editing,        // Modal form
    Filtering,      // Launcher filter
    AdHocCmd,       // Ad-hoc command
    
    // Search Specific Modes
    SearchInput,    // Typing query
    SearchSidebar,  // Navigating history/settings
    ChatHistory,    // Scrolling chat
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchSidebarState {
    Hidden,
    History,
    Settings,
}

#[derive(Debug, Clone)]
pub enum AppAction {
    Tick,
    Quit,
    SwitchTab,
    
    // Launcher
    LoadApps,
    AppsLoaded(Vec<AppModel>),
    SelectNext,
    SelectPrev,
    ToggleFilter,
    EnterFilterChar(char),
    BackspaceFilter,
    OpenAddModal,
    OpenEditModal,
    ConfirmDelete,
    CloseModal,
    CycleFormFocus,
    FormChar(char),
    FormBackspace,
    SubmitForm,
    LaunchSelected,
    LaunchResult(String),
    OpenAdHocModal,
    SubmitAdHoc(String),
    
    // Search Actions
    ToggleSearchSidebar,
    CycleSearchFocus,
    SidebarNext,
    SidebarPrev,
    SidebarSelect,
    NewConversation,
    
    // Search Data Loading
    LoadSearchState,
    ConversationsLoaded(Vec<Conversation>),
    ProvidersLoaded(Vec<ProviderConfig>),
    ModelsLoaded(Vec<Model>),
    ConversationCreated(i64),
    LoadConversation(i64),
    ConversationLoaded(Value),
    
    // Search Interaction
    EnterSearchChar(char),
    DeleteSearchChar,
    SubmitSearch,
    ScrollChat(i16),
    SearchSourcesReceived(Vec<SearchSource>),
    SearchStreamToken(String),
    SearchError(String),
    SearchDone,
}

#[derive(Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub sources: Vec<SearchSource>,
}

#[derive(Debug, Clone)]
pub struct AppForm {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub cmd: String,
    pub url: String,
    pub focus_idx: usize,
}
impl Default for AppForm {
    fn default() -> Self {
        Self {
            id: String::new(), name: String::new(), desc: String::new(), cmd: String::new(), url: "http://localhost".into(), focus_idx: 0,
        }
    }
}

pub struct App {
    pub should_quit: bool,
    pub current_screen: CurrentScreen,
    pub input_mode: InputMode,
    
    // --- Launcher State ---
    pub apps: Vec<AppModel>,
    pub filtered_apps: Vec<usize>,
    pub apps_idx: usize,
    pub launcher_logs: Vec<String>,
    pub is_loading_apps: bool,
    pub filter_input: String,
    pub active_form: AppForm,
    pub adhoc_input: String,

    // --- Searchrs State ---
    pub search_input: String,
    pub messages: Vec<ChatMessage>,
    pub is_searching: bool,
    pub search_sidebar: SearchSidebarState,
    
    pub chat_scroll: u16,
    pub chat_auto_scroll: bool,
    
    // Search Config Data
    pub current_convo_id: Option<i64>,
    pub conversations: Vec<Conversation>,
    pub conversation_idx: usize,
    
    pub llm_providers: Vec<String>,
    pub selected_llm_provider: String,
    
    pub models: Vec<Model>,
    pub selected_model: String,
    
    pub search_providers: Vec<ProviderConfig>,
    pub settings_idx: usize,
    
    pub action_tx: mpsc::UnboundedSender<AppAction>,
    pub action_rx: mpsc::UnboundedReceiver<AppAction>,
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            should_quit: false,
            current_screen: CurrentScreen::Launcher,
            input_mode: InputMode::Normal,
            
            // Launcher Defaults
            apps: vec![], filtered_apps: vec![], apps_idx: 0,
            launcher_logs: vec!["Ready.".into()], is_loading_apps: false,
            filter_input: String::new(), active_form: AppForm::default(), adhoc_input: String::new(),

            // Search Defaults
            search_input: String::new(),
            messages: vec![ChatMessage { 
                role: "system".into(), 
                content: "Welcome to bplus search.\n\n- Press **Tab** to cycle focus (Sidebar -> Chat -> Input).\n- Use **Up/Down/PgUp/PgDn** to scroll chat when focused.".into(),
                sources: vec![]
            }],
            is_searching: false,
            search_sidebar: SearchSidebarState::Hidden,
            chat_scroll: 0,
            chat_auto_scroll: true,
            
            current_convo_id: None,
            conversations: vec![],
            conversation_idx: 0,
            
            llm_providers: vec!["lmstudio".into(), "openai".into(), "openrouter".into(), "google".into()],
            selected_llm_provider: "lmstudio".into(),
            
            models: vec![],
            selected_model: "Loading...".into(),
            
            search_providers: vec![],
            settings_idx: 0,

            action_tx: tx,
            action_rx: rx,
        }
    }

    pub fn get_selected_app(&self) -> Option<&AppModel> {
        if self.filtered_apps.is_empty() { return None; }
        self.apps.get(*self.filtered_apps.get(self.apps_idx)?)
    }

    fn update_filter(&mut self) {
        let query = self.filter_input.to_lowercase();
        self.filtered_apps = self.apps.iter().enumerate()
            .filter(|(_, app)| {
                if query.is_empty() { return true; }
                app.name.to_lowercase().contains(&query) || 
                app.description.clone().unwrap_or_default().to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
        self.apps_idx = 0;
    }

    pub async fn update(&mut self, action: AppAction) {
        match action {
            AppAction::Tick => {},
            AppAction::Quit => self.should_quit = true,
            AppAction::SwitchTab => {
                if self.input_mode == InputMode::Editing { return; }

                self.current_screen = match self.current_screen {
                    CurrentScreen::Launcher => {
                        self.input_mode = InputMode::SearchInput; 
                        if self.search_providers.is_empty() {
                            let _ = self.action_tx.send(AppAction::LoadSearchState);
                        }
                        CurrentScreen::Search
                    },
                    CurrentScreen::Search => {
                        self.input_mode = InputMode::Normal;
                        CurrentScreen::Launcher
                    },
                };
            },

            // --- LAUNCHER LOGIC ---
            AppAction::SelectNext => { if !self.filtered_apps.is_empty() { self.apps_idx = (self.apps_idx + 1) % self.filtered_apps.len(); } },
            AppAction::SelectPrev => { if !self.filtered_apps.is_empty() { if self.apps_idx == 0 { self.apps_idx = self.filtered_apps.len() - 1; } else { self.apps_idx -= 1; } } },
            AppAction::LoadApps => {
                self.is_loading_apps = true;
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match api::fetch_apps().await {
                        Ok(apps) => tx.send(AppAction::AppsLoaded(apps)).unwrap(),
                        Err(e) => tx.send(AppAction::LaunchResult(format!("Error fetching apps: {}", e))).unwrap(),
                    }
                });
            },
            AppAction::AppsLoaded(apps) => { self.apps = apps; self.is_loading_apps = false; self.update_filter(); },
            AppAction::ToggleFilter => {
                self.input_mode = match self.input_mode {
                    InputMode::Filtering => InputMode::Normal,
                    _ => { self.filter_input.clear(); self.update_filter(); InputMode::Filtering },
                };
            },
            AppAction::EnterFilterChar(c) => { self.filter_input.push(c); self.update_filter(); },
            AppAction::BackspaceFilter => { self.filter_input.pop(); self.update_filter(); },
            
            AppAction::OpenAddModal => { self.active_form = AppForm::default(); self.input_mode = InputMode::Editing; },
            AppAction::OpenEditModal => { if let Some(app) = self.get_selected_app() { self.active_form = AppForm { id: app.id.clone(), name: app.name.clone(), desc: app.description.clone().unwrap_or_default(), cmd: app.command.clone(), url: app.url.clone(), focus_idx: 0 }; self.input_mode = InputMode::Editing; } },
            AppAction::CloseModal => { self.input_mode = InputMode::Normal; },
            AppAction::CycleFormFocus => { self.active_form.focus_idx = (self.active_form.focus_idx + 1) % 4; },
            AppAction::FormChar(c) => match self.active_form.focus_idx { 0=>self.active_form.name.push(c),1=>self.active_form.desc.push(c),2=>self.active_form.cmd.push(c),3=>self.active_form.url.push(c),_=>{} },
            AppAction::FormBackspace => match self.active_form.focus_idx { 0=>{self.active_form.name.pop();},1=>{self.active_form.desc.pop();},2=>{self.active_form.cmd.pop();},3=>{self.active_form.url.pop();},_=>{} },
            AppAction::SubmitForm => {
                let form = self.active_form.clone();
                let model = AppModel { id: form.id.clone(), name: form.name, description: Some(form.desc), command: form.cmd, url: form.url };
                let tx = self.action_tx.clone();
                self.input_mode = InputMode::Normal;
                tokio::spawn(async move {
                    let res = if form.id.is_empty() { api::create_app(&model).await } else { api::update_app(&model).await.map(|_| model) };
                    match res { Ok(_) => { tx.send(AppAction::LoadApps).unwrap(); }, Err(e) => tx.send(AppAction::LaunchResult(format!("Error: {}", e))).unwrap() }
                });
            },
            AppAction::ConfirmDelete => { if let Some(app) = self.get_selected_app() { let id = app.id.clone(); let tx = self.action_tx.clone(); tokio::spawn(async move { let _ = api::delete_app(&id).await; tx.send(AppAction::LoadApps).unwrap(); }); } },
            
            AppAction::LaunchSelected => {
                if let Some(app) = self.get_selected_app() {
                    let id = app.id.clone();
                    let name = app.name.clone();
                    let tx = self.action_tx.clone();
                    
                    self.launcher_logs.push(format!("Executing '{}'...", name));
                    
                    tokio::spawn(async move {
                        match api::launch_app(id).await {
                            Ok(res) => {
                                let output = if res.success {
                                    format!("Success:\n{}", res.stdout)
                                } else {
                                    format!("Failed:\n{}\n{}", res.message, res.stderr)
                                };
                                tx.send(AppAction::LaunchResult(output)).unwrap();
                            },
                            Err(e) => {
                                tx.send(AppAction::LaunchResult(format!("API Error: {}", e))).unwrap();
                            }
                        }
                    });
                }
            },
            
            AppAction::LaunchResult(msg) => { for line in msg.lines() { self.launcher_logs.push(line.to_string()); } if self.launcher_logs.len() > 100 { let r = self.launcher_logs.len()-100; self.launcher_logs.drain(0..r); } },
            
            AppAction::OpenAdHocModal => { self.adhoc_input.clear(); self.input_mode = InputMode::AdHocCmd; },
            
            AppAction::SubmitAdHoc(cmd) => {
                self.input_mode = InputMode::Normal;
                let tx = self.action_tx.clone();
                self.launcher_logs.push(format!("Running ad-hoc: {}", cmd));
                
                tokio::spawn(async move {
                    let temp_app = AppModel {
                        id: String::new(),
                        name: "__TEMP_CMD__".into(),
                        description: Some("Ad-hoc".into()),
                        command: cmd,
                        url: "http://localhost".into(),
                    };
                    
                    match api::create_app(&temp_app).await {
                        Ok(created) => {
                            let launch_res = api::launch_app(created.id.clone()).await;
                            let _ = api::delete_app(&created.id).await;
                            
                            match launch_res {
                                Ok(res) => {
                                     let output = if res.success {
                                        format!("{}\n{}", res.stdout, res.stderr)
                                    } else {
                                        format!("Failed: {}\n{}", res.message, res.stderr)
                                    };
                                    tx.send(AppAction::LaunchResult(output)).unwrap();
                                },
                                Err(e) => tx.send(AppAction::LaunchResult(format!("Exec Error: {}", e))).unwrap(),
                            }
                        },
                        Err(e) => tx.send(AppAction::LaunchResult(format!("AdHoc Error: {}", e))).unwrap(),
                    }
                });
            },

            // --- SEARCH LOGIC ---
            AppAction::LoadSearchState => {
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    if let Ok(convos) = api::fetch_conversations().await { tx.send(AppAction::ConversationsLoaded(convos)).unwrap(); }
                    if let Ok(provs) = api::fetch_providers_list().await { tx.send(AppAction::ProvidersLoaded(provs)).unwrap(); }
                });
                let tx2 = self.action_tx.clone();
                let prov = self.selected_llm_provider.clone();
                tokio::spawn(async move {
                    if let Ok(models) = api::fetch_models(&prov).await { tx2.send(AppAction::ModelsLoaded(models)).unwrap(); }
                });
            },
            AppAction::ConversationsLoaded(convos) => { self.conversations = convos; },
            AppAction::ProvidersLoaded(provs) => { self.search_providers = provs; },
            AppAction::ModelsLoaded(models) => { 
                self.models = models;
                if let Some(first) = self.models.first() { self.selected_model = first.id.clone(); }
                else { self.selected_model = "default".into(); }
            },
            AppAction::ToggleSearchSidebar => {
                self.search_sidebar = match self.search_sidebar {
                    SearchSidebarState::Hidden => SearchSidebarState::History,
                    SearchSidebarState::History => SearchSidebarState::Settings,
                    SearchSidebarState::Settings => SearchSidebarState::Hidden,
                };
                if self.search_sidebar != SearchSidebarState::Hidden {
                    self.input_mode = InputMode::SearchSidebar;
                } else {
                    self.input_mode = InputMode::SearchInput;
                }
            },
            AppAction::CycleSearchFocus => {
                self.input_mode = match self.input_mode {
                    InputMode::SearchInput => {
                         if self.search_sidebar != SearchSidebarState::Hidden {
                             InputMode::SearchSidebar
                         } else {
                             InputMode::ChatHistory
                         }
                    },
                    InputMode::SearchSidebar => InputMode::ChatHistory,
                    InputMode::ChatHistory => InputMode::SearchInput,
                    _ => InputMode::SearchInput,
                };
            },
            AppAction::SidebarNext => {
                match self.search_sidebar {
                    SearchSidebarState::History => {
                        let max = self.conversations.len() + 1; 
                        self.conversation_idx = (self.conversation_idx + 1) % max;
                    },
                    SearchSidebarState::Settings => { self.settings_idx = (self.settings_idx + 1) % (2 + self.search_providers.len()); },
                    _ => {}
                }
            },
            AppAction::SidebarPrev => {
                match self.search_sidebar {
                    SearchSidebarState::History => {
                        let max = self.conversations.len() + 1;
                        if self.conversation_idx == 0 { self.conversation_idx = max - 1; } else { self.conversation_idx -= 1; }
                    },
                    SearchSidebarState::Settings => { if self.settings_idx == 0 { self.settings_idx = (2 + self.search_providers.len()) - 1; } else { self.settings_idx -= 1; } },
                    _ => {}
                }
            },
            AppAction::SidebarSelect => {
                match self.search_sidebar {
                    SearchSidebarState::History => {
                        if self.conversation_idx == 0 {
                            self.action_tx.send(AppAction::NewConversation).unwrap();
                        } else if let Some(c) = self.conversations.get(self.conversation_idx - 1) {
                             self.action_tx.send(AppAction::LoadConversation(c.id)).unwrap();
                        }
                    },
                    SearchSidebarState::Settings => {
                        if self.settings_idx == 0 {
                            let curr_pos = self.llm_providers.iter().position(|p| p == &self.selected_llm_provider).unwrap_or(0);
                            let next_pos = (curr_pos + 1) % self.llm_providers.len();
                            self.selected_llm_provider = self.llm_providers[next_pos].clone();
                            let tx = self.action_tx.clone(); let p = self.selected_llm_provider.clone();
                            tokio::spawn(async move { if let Ok(m) = api::fetch_models(&p).await { tx.send(AppAction::ModelsLoaded(m)).unwrap(); } });
                        } else if self.settings_idx == 1 {
                            if !self.models.is_empty() {
                                let curr = self.models.iter().position(|m| m.id == self.selected_model).unwrap_or(0);
                                let next = (curr + 1) % self.models.len();
                                self.selected_model = self.models[next].id.clone();
                            }
                        } else {
                            if let Some(p) = self.search_providers.get_mut(self.settings_idx - 2) { p.is_enabled = !p.is_enabled; }
                        }
                    },
                    _ => {}
                }
            },
            AppAction::NewConversation => {
                self.current_convo_id = None;
                self.messages.clear();
                self.messages.push(ChatMessage { role: "system".into(), content: "New conversation started.".into(), sources: vec![] });
                self.chat_auto_scroll = true;
                self.search_sidebar = SearchSidebarState::Hidden;
                self.input_mode = InputMode::SearchInput;
            },
            AppAction::ConversationCreated(id) => {
                self.current_convo_id = Some(id);
                let tx = self.action_tx.clone();
                tokio::spawn(async move { if let Ok(c) = api::fetch_conversations().await { tx.send(AppAction::ConversationsLoaded(c)).unwrap(); } });
            },
            AppAction::LoadConversation(id) => {
                self.current_convo_id = Some(id);
                self.messages.clear();
                self.messages.push(ChatMessage { role: "system".into(), content: "Loading conversation...".into(), sources: vec![] });
                self.chat_auto_scroll = true;
                self.input_mode = InputMode::ChatHistory; // Focus chat so user can see it loading
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    if let Ok(json) = api::load_conversation(id).await { 
                        tx.send(AppAction::ConversationLoaded(json)).unwrap(); 
                    } else {
                        tx.send(AppAction::SearchError("Failed to load chat".into())).unwrap();
                    }
                });
            },
            AppAction::ConversationLoaded(json) => {
                self.messages.clear();
                if let Some(msgs) = json["messages"].as_array() {
                    for m in msgs {
                        let role = m["role"].as_str().unwrap_or("unknown").to_string();
                        let content = m["content"].as_str().unwrap_or("").to_string();
                        let sources: Vec<SearchSource> = if let Some(s_str) = m["sources"].as_str() { serde_json::from_str(s_str).unwrap_or_default() } else { vec![] };
                        self.messages.push(ChatMessage { role, content, sources });
                    }
                }
                self.chat_auto_scroll = true;
            },
            AppAction::ScrollChat(delta) => {
                self.chat_auto_scroll = false;
                if delta < 0 {
                    self.chat_scroll = self.chat_scroll.saturating_sub(delta.abs() as u16);
                } else {
                    self.chat_scroll = self.chat_scroll.saturating_add(delta as u16);
                }
            },
            AppAction::EnterSearchChar(c) => self.search_input.push(c),
            AppAction::DeleteSearchChar => { self.search_input.pop(); },
            AppAction::SubmitSearch => {
                if !self.search_input.trim().is_empty() && !self.is_searching {
                    let query = self.search_input.clone();
                    self.messages.push(ChatMessage { role: "user".into(), content: query.clone(), sources: vec![] });
                    self.messages.push(ChatMessage { role: "assistant".into(), content: String::new(), sources: vec![] });
                    self.search_input.clear();
                    self.is_searching = true;
                    self.chat_auto_scroll = true;
                    
                    let tx = self.action_tx.clone();
                    let convo_id = self.current_convo_id;
                    let model = self.selected_model.clone();
                    let prov = self.selected_llm_provider.clone();
                    let active_prov_ids: Vec<i64> = self.search_providers.iter().filter(|p| p.is_enabled).map(|p| p.id).collect();
                    tokio::spawn(async move {
                        if let Err(e) = api::start_search_stream(query, convo_id, model, prov, active_prov_ids, tx.clone()).await {
                            tx.send(AppAction::SearchError(e.to_string())).unwrap();
                        }
                    });
                }
            },
            AppAction::SearchSourcesReceived(sources) => { if let Some(last) = self.messages.last_mut() { if last.role == "assistant" { last.sources = sources; } } },
            AppAction::SearchStreamToken(text) => { if let Some(last) = self.messages.last_mut() { if last.role == "assistant" { last.content.push_str(&text); } } },
            AppAction::SearchError(err) => { self.messages.push(ChatMessage { role: "system".into(), content: format!("Error: {}", err), sources: vec![] }); self.is_searching = false; },
            AppAction::SearchDone => { self.is_searching = false; },
        }
    }
}