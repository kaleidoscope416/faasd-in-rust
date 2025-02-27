use crate::types::*;
use actix_web::{HttpResponse, Responder, web};
use service::Service;
use std::sync::Arc;

/// 创建并启动容器
pub async fn create_container(
    service: web::Data<Arc<Service>>,
    info: web::Json<CreateContainerInfo>,
) -> impl Responder {
    let cid = info.container_id.clone();
    let image = info.image.clone();
    let ns = info.ns.clone();
    service.create_container(&image, &cid, &ns).await.unwrap();
    HttpResponse::Ok().json("Container created successfully!")
}

/// 删除容器
pub async fn remove_container(
    service: web::Data<Arc<Service>>,
    info: web::Json<RemoveContainerInfo>,
) -> impl Responder {
    let container_id = info.container_id.clone();
    let ns = info.ns.clone();
    service.remove_container(&container_id, &ns).await.unwrap();
    HttpResponse::Ok().json("Container removed successfully!")
}

/// 获取容器列表
pub async fn get_container_list(
    service: web::Data<Arc<Service>>,
    info: web::Json<GetContainerListQuery>,
) -> impl Responder {
    let ns = info.ns.clone();
    let container_list = service.get_container_list(&ns).await.unwrap();
    HttpResponse::Ok().json(container_list)
}

// 添加更多的路由处理函数...
