use containerd_client::{
    services::v1::{
        container::Runtime, Container, CreateContainerRequest, CreateTaskRequest,
        DeleteContainerRequest, DeleteTaskRequest, KillRequest, ListContainersRequest,
        ListTasksRequest, StartRequest, WaitRequest,
    },
    tonic::Request,
    with_namespace, Client,
};
use std::{
    fs::{self, File},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::timeout;

// config.json,dockerhub密钥
const DOCKER_CONFIG_DIR: &str = "/var/lib/faasd/.docker/";
// 命名空间（容器的）
const NAMESPACE: &str = "default";

type Err = Box<dyn std::error::Error>;

pub struct Service {
    client: Arc<Mutex<Client>>,
}

impl Service {
    pub async fn new(endpoint: String) -> Result<Self, Err> {
        let client = Client::from_path(endpoint).await.unwrap();
        Ok(Service {
            client: Arc::new(Mutex::new(client)),
        })
    }

    pub async fn create_container(&self, image: String, cid: String) {
        let mut containers_client = self.client.lock().unwrap().containers();
        let container = Container {
            id: cid.to_string(),
            image,
            runtime: Some(Runtime {
                name: "io.containerd.runc.v2".to_string(),
                options: None,
            }),
            spec: None,
            ..Default::default()
        };

        let req = CreateContainerRequest {
            container: Some(container),
        };

        let req = with_namespace!(req, NAMESPACE);

        let _resp = containers_client
            .create(req)
            .await
            .expect("Failed to create container");

        println!("Container: {:?} created", cid);

        self.create_and_start_task(cid).await;
    }

    pub async fn remove_container(&self, container_id: String) {
        let c = self.client.lock().unwrap();
        let mut containers_client = c.containers();
        let request = Request::new(ListContainersRequest {
            ..Default::default()
        });

        let responce = containers_client.list(request).await.unwrap().into_inner();
        let container = responce
            .containers
            .iter()
            .find(|container| container.id == container_id);

        if let Some(container) = container {
            let mut tasks_client = c.tasks();

            let request = Request::new(ListTasksRequest {
                filter: format!("container=={}", container_id),
                ..Default::default()
            });
            let responce = tasks_client.list(request).await.unwrap().into_inner();
            drop(tasks_client);
            if let Some(task) = responce
                .tasks
                .iter()
                .find(|task| task.container_id == container.id)
            {
                println!("Task found: {}, Status: {}", task.id, task.status);
                // TASK_UNKNOWN (0) — 未知状态
                // TASK_CREATED (1) — 任务已创建
                // TASK_RUNNING (2) — 任务正在运行
                // TASK_STOPPED (3) — 任务已停止
                // TASK_EXITED (4) — 任务已退出
                // TASK_PAUSED (5) — 任务已暂停
                // TASK_FAILED (6) — 任务失败
                self.delete_task(&task.container_id).await;
            }

            let delete_request = DeleteContainerRequest {
                id: container.id.clone(),
                ..Default::default()
            };
            let delete_request = with_namespace!(delete_request, NAMESPACE);

            let _ = containers_client
                .delete(delete_request)
                .await
                .expect("Failed to delete container");

            println!("Container: {:?} deleted", containers_client);
        } else {
            todo!("Container not found");
        }
        drop(containers_client);
    }

    pub async fn create_and_start_task(&self, container_id: String) {
        let tmp = std::env::temp_dir().join("containerd-client-test");
        println!("Temp dir: {:?}", tmp);
        fs::create_dir_all(&tmp).expect("Failed to create temp directory");
        let stdin = tmp.join("stdin");
        let stdout = tmp.join("stdout");
        let stderr = tmp.join("stderr");
        File::create(&stdin).expect("Failed to create stdin");
        File::create(&stdout).expect("Failed to create stdout");
        File::create(&stderr).expect("Failed to create stderr");

        let mut tasks_client = self.client.lock().unwrap().tasks();

        let req = CreateTaskRequest {
            container_id: container_id.clone(),
            stdin: stdin.to_str().unwrap().to_string(),
            stdout: stdout.to_str().unwrap().to_string(),
            stderr: stderr.to_str().unwrap().to_string(),
            ..Default::default()
        };
        let req = with_namespace!(req, NAMESPACE);

        let _resp = tasks_client
            .create(req)
            .await
            .expect("Failed to create task");

        println!("Task: {:?} created", container_id);

        let req = StartRequest {
            container_id: container_id.to_string(),
            ..Default::default()
        };
        let req = with_namespace!(req, NAMESPACE);

        let _resp = tasks_client.start(req).await.expect("Failed to start task");

        println!("Task: {:?} started", container_id);
    }

    pub async fn delete_task(&self, container_id: &str) {
        let time_out = Duration::from_secs(30);
        let mut tc = self.client.lock().unwrap().tasks();
        let wait_result = timeout(time_out, async {
            let wait_request = Request::new(WaitRequest {
                container_id: container_id.to_string(),
                ..Default::default()
            });

            let _ = tc.wait(wait_request).await?;
            Ok::<(), Err>(())
        })
        .await;

        let kill_request = Request::new(KillRequest {
            container_id: container_id.to_string(),
            signal: 15,
            all: true,
            ..Default::default()
        });
        tc.kill(kill_request).await.expect("Failed to kill task");

        match wait_result {
            Ok(Ok(_)) => {
                let req = DeleteTaskRequest {
                    container_id: container_id.to_string(),
                };
                let req = with_namespace!(req, NAMESPACE);

                let _resp = tc.delete(req).await.expect("Failed to delete task");
                println!("Task: {:?} deleted", container_id);
            }
            _ => {
                let kill_request = Request::new(KillRequest {
                    container_id: container_id.to_string(),
                    signal: 9,
                    all: true,
                    ..Default::default()
                });
                tc.kill(kill_request)
                    .await
                    .expect("Failed to FORCE kill task");
            }
        }
    }

    pub async fn get_container_list(&self) -> Result<Vec<String>, tonic::Status> {
        let mut cc = self.client.lock().unwrap().containers();

        let request = ListContainersRequest {
            ..Default::default()
        };

        let request = with_namespace!(request, NAMESPACE);

        let response = cc.list(request).await?;

        Ok(response
            .into_inner()
            .containers
            .into_iter()
            .map(|container| container.id)
            .collect())
    }

    pub fn prepare_image(&self) {
        todo!()
    }
    pub fn pull_image(&self) {
        todo!()
    }
    /// 获取resolver，验证用，后面拉取镜像可能会用到
    pub fn get_resolver(&self) {
        todo!()
    }
}
