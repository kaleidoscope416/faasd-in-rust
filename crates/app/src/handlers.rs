use crate::types::*;
use actix_web::{web, HttpResponse, Responder};
use service::Service;
use std::sync::Arc;

/// 创建并启动容器
pub async fn create_container(
    service: web::Data<Arc<Service>>,
    info: web::Json<CreateContainerInfo>,
) -> impl Responder {
    let cid = info.container_id.clone();
    let image = info.image.clone();
    service.create_container(image, cid).await;
    HttpResponse::Ok().json("Container created successfully!")
}

/// 删除容器
pub async fn remove_container(
    service: web::Data<Arc<Service>>,
    info: web::Json<RemoveContainerInfo>,
) -> impl Responder {
    let container_id = info.container_id.clone();
    service.remove_container(container_id).await;
    HttpResponse::Ok().json("Container removed successfully!")
}

pub async fn get_container_list(service: web::Data<Arc<Service>>) -> impl Responder {
    let container_list = service.get_container_list().await.unwrap();
    HttpResponse::Ok().json(container_list)
}

// 添加更多的路由处理函数...
