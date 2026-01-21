#[derive(Clone, Debug)]
pub(crate) struct Template {
    pub(crate) name: String,
    pub(crate) body: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TreeItem {
    pub(crate) label: String,
    pub(crate) depth: usize,
    pub(crate) template_index: Option<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) value: String,
}

#[derive(Clone, Debug)]
pub(crate) enum Token {
    Text(String),
    Var {
        name: String,
        desc: Option<String>,
        raw: String,
    },
    Random {
        options: Vec<String>,
        choice: String,
        raw: String,
    },
}
