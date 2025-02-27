use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct CreateContainerInfo {
    pub container_id: String,
    pub image: String,
    pub ns: String,
}

#[derive(Serialize, Deserialize)]
pub struct RemoveContainerInfo {
    pub container_id: String,
    pub ns: String,
}

#[derive(Deserialize)]
pub struct GetContainerListQuery {
    pub status: Option<String>,
    pub ns: String,
}