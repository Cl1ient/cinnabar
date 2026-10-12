// Generated from crates/server-experience/schema/extension.json; do not edit.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Channel {
    pub id: String,
    pub schema: u16,
    pub direction: Direction,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub version: u16,
    pub session: String,
    pub connection: String,
    pub subclient: u8,
    pub bundle: String,
    pub generation: u64,
    pub channel: String,
    pub schema: u16,
    pub sequence: u64,
    pub world_epoch: u64,
    pub payload: Vec<Scalar>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub index: u32,
    pub count: u32,
    pub data: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Fragment {
    pub version: u16,
    pub session: String,
    pub connection: String,
    pub subclient: u8,
    pub bundle: String,
    pub generation: u64,
    pub channel: String,
    pub schema: u16,
    pub sequence: u64,
    pub world_epoch: u64,
    pub fragment: Part,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    ToClient,
    ToServer,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(
    rename_all = "snake_case",
    tag = "type",
    deny_unknown_fields,
    content = "value"
)]
pub enum Scalar {
    Bool(bool),
    Integer(i64),
    Text(String),
    Choice(u16),
    List(Vec<Scalar>),
    Record(Vec<Scalar>),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", tag = "type", deny_unknown_fields)]
pub enum Field {
    Bool {},
    Integer { min: i64, max: i64 },
    Text { max_bytes: u16 },
    Choice { variants: u16 },
    List { item: Box<Field>, max_items: u16 },
    Record { fields: Vec<Field> },
}

impl Field {
    /// Checks container nesting and record field counts before accepting a declaration.
    fn declared(&self, depth: usize) -> bool {
        match self {
            Field::List { item, .. } => depth > 0 && item.declared(depth - 1),
            Field::Record { fields } => {
                depth > 0
                    && fields.len() <= MAX_CHANNEL_FIELDS
                    && fields.iter().all(|field| field.declared(depth - 1))
            }
            _ => true,
        }
    }
    /// Checks the value's type and every schema-defined bound recursively.
    fn admits(&self, value: &Scalar) -> bool {
        match (self, value) {
            (Field::Bool, Scalar::Bool(_)) => true,
            (Field::Integer { min, max }, Scalar::Integer(value)) => min <= value && value <= max,
            (Field::Text { max_bytes }, Scalar::Text(value)) => {
                value.len() <= usize::from(*max_bytes)
            }
            (Field::Choice { variants }, Scalar::Choice(value)) => value < variants,
            (Field::List { item, max_items }, Scalar::List(items)) => {
                items.len() <= usize::from(*max_items)
                    && items.iter().all(|value| item.admits(value))
            }
            (Field::Record { fields }, Scalar::Record(values)) => {
                fields.len() == values.len()
                    && fields
                        .iter()
                        .zip(values)
                        .all(|(field, value)| field.admits(value))
            }
            _ => false,
        }
    }
}
impl Channel {
    /// Checks identifiers, field counts and nesting against the shared ceilings.
    pub fn declared(&self) -> bool {
        identifier(&self.id)
            && self.fields.len() <= MAX_CHANNEL_FIELDS
            && self
                .fields
                .iter()
                .all(|field| field.declared(MAX_FIELD_DEPTH))
    }
    /// Checks the record's direction, positional fields and encoded byte ceiling.
    pub fn validate(
        &self,
        payload: &[Scalar],
        direction: Direction,
        max_bytes: usize,
    ) -> Result<()> {
        ensure!(
            self.declared() && self.direction == direction,
            "channel denied"
        );
        ensure!(
            payload.len() == self.fields.len(),
            "record field count mismatch"
        );
        for (field, value) in self.fields.iter().zip(payload) {
            ensure!(field.admits(value), "record field rejected");
        }
        ensure!(
            serde_json::to_vec(payload)?.len() <= max_bytes,
            "payload too large"
        );
        Ok(())
    }
}
