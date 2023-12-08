use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Intent {
    pub action: Option<String>,
    pub data: Option<String>,
    pub mime_type: Option<String>,
    pub category: Option<String>,
    pub component: Option<String>,
    pub package: Option<String>,
    pub user_id: Option<String>,
    pub flags: u32,
    pub receiver_foreground: bool,
    pub wait: bool,
    pub extra: Extra,
}

#[derive(Debug, Default)]
pub struct Extra {
    pub es: HashMap<String, String>,
    pub ez: HashMap<String, bool>,
    pub ei: HashMap<String, i32>,
    pub el: HashMap<String, i64>,
    pub ef: HashMap<String, f32>,
    pub eu: HashMap<String, String>,
    pub ecn: HashMap<String, String>,
    pub eia: HashMap<String, Vec<i32>>,
    pub ela: HashMap<String, Vec<i64>>,
    pub efa: HashMap<String, Vec<f32>>,
    pub esa: HashMap<String, Vec<String>>,
    pub grant_read_uri_permission: bool,
    pub grant_write_uri_permission: bool,
    pub exclude_stopped_packages: bool,
    pub include_stopped_packages: bool,
}
