pub mod spec;

use containerd_client::{
    services::v1::{
        container::Runtime,
        snapshots::{MountsRequest, PrepareSnapshotRequest},
        Container, CreateContainerRequest, CreateTaskRequest, DeleteContainerRequest,
        DeleteTaskRequest, GetImageRequest, KillRequest, ListContainersRequest, ListTasksRequest,
        ReadContentRequest, StartRequest, WaitRequest,
    },
    tonic::Request,
    types::Mount,
    with_namespace, Client,
};
use oci_spec::image::{Arch, ImageConfiguration, ImageIndex, ImageManifest, MediaType, Os};
use prost_types::Any;
use sha2::{Digest, Sha256};
use spec::generate_spec;
use std::{
    fs,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::timeout;

// config.json,dockerhub密钥
// const DOCKER_CONFIG_DIR: &str = "/var/lib/faasd/.docker/";
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

    async fn prepare_snapshot(&self, cid: &str, ns: &str) -> Result<Vec<Mount>, Err> {
        let parent_snapshot = self.get_parent_snapshot(cid, ns).await?;
        let req = PrepareSnapshotRequest {
            snapshotter: "overlayfs".to_string(),
            key: cid.to_string(),
            parent: parent_snapshot,
            ..Default::default()
        };
        let resp = self
            .client
            .lock()
            .unwrap()
            .snapshots()
            .prepare(with_namespace!(req, ns))
            .await?
            .into_inner()
            .mounts;

        Ok(resp)
    }

    pub async fn create_container(
        &self,
        image_name: &str,
        cid: &str,
        ns: &str,
    ) -> Result<(), Err> {
        let namespace = match ns {
            "" => spec::DEFAULT_NAMESPACE,
            _ => ns,
        };

        let _mount = self.prepare_snapshot(cid, ns).await?;

        let spec_path = generate_spec(&cid, ns).unwrap();
        let spec = fs::read_to_string(spec_path).unwrap();

        let spec = Any {
            type_url: "types.containerd.io/opencontainers/runtime-spec/1/Spec".to_string(),
            value: spec.into_bytes(),
        };

        let mut containers_client = self.client.lock().unwrap().containers();
        let container = Container {
            id: cid.to_string(),
            image: image_name.to_string(),
            runtime: Some(Runtime {
                name: "io.containerd.runc.v2".to_string(),
                options: None,
            }),
            spec: Some(spec),
            snapshotter: "overlayfs".to_string(),
            snapshot_key: cid.to_string(),
            ..Default::default()
        };

        let req = CreateContainerRequest {
            container: Some(container),
        };

        let req = with_namespace!(req, namespace);

        let _resp = containers_client
            .create(req)
            .await
            .expect("Failed to create container");

        // println!("Container: {:?} created", cid);
        Ok(())
    }

    pub async fn remove_container(&self, cid: &str, ns: &str) -> Result<(), Err> {
        let namespace = match ns {
            "" => NAMESPACE,
            _ => ns,
        };
        let c = self.client.lock().unwrap();
        let request = ListContainersRequest {
            ..Default::default()
        };
        let mut cc = c.containers();

        let responce = cc
            .list(with_namespace!(request, namespace))
            .await?
            .into_inner();
        let container = responce
            .containers
            .iter()
            .find(|container| container.id == cid);

        if let Some(container) = container {
            let mut tc = c.tasks();

            let request = ListTasksRequest {
                filter: format!("container=={}", cid),
                ..Default::default()
            };
            let responce = tc
                .list(with_namespace!(request, namespace))
                .await?
                .into_inner();
            drop(tc);
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
                self.delete_task(&task.container_id, ns).await;
            }

            let delete_request = DeleteContainerRequest {
                id: container.id.clone(),
                ..Default::default()
            };

            let _ = cc
                .delete(with_namespace!(delete_request, namespace))
                .await
                .expect("Failed to delete container");

            // println!("Container: {:?} deleted", cc);
        } else {
            todo!("Container not found");
        }
        drop(cc);
        Ok(())
    }

    pub async fn create_and_start_task(&self, cid: &str, ns: &str) -> Result<(), Err> {
        // let tmp = std::env::temp_dir().join("containerd-client-test");
        // println!("Temp dir: {:?}", tmp);
        // fs::create_dir_all(&tmp).expect("Failed to create temp directory");
        // let stdin = tmp.join("stdin");
        // let stdout = tmp.join("stdout");
        // let stderr = tmp.join("stderr");
        // File::create(&stdin).expect("Failed to create stdin");
        // File::create(&stdout).expect("Failed to create stdout");
        // File::create(&stderr).expect("Failed to create stderr");

        let namespace = match ns {
            "" => spec::DEFAULT_NAMESPACE,
            _ => ns,
        };
        self.create_task(cid, namespace).await?;
        self.start_task(cid, namespace).await?;
        Ok(())
    }

    async fn create_task(&self, cid: &str, ns: &str) -> Result<(), Err> {
        let c = self.client.lock().unwrap();
        let mut sc = c.snapshots();
        let req = MountsRequest {
            snapshotter: "overlayfs".to_string(),
            key: cid.to_string(),
        };
        let mounts = sc
            .mounts(with_namespace!(req, ns))
            .await?
            .into_inner()
            .mounts;
        drop(sc);
        let mut tc = c.tasks();
        let req = CreateTaskRequest {
            container_id: cid.to_string(),
            rootfs: mounts,
            ..Default::default()
        };
        let _resp = tc.create(with_namespace!(req, ns)).await?;

        Ok(())
    }

    async fn start_task(&self, cid: &str, ns: &str) -> Result<(), Err> {
        let req = StartRequest {
            container_id: cid.to_string(),
            ..Default::default()
        };
        let _resp = self
            .client
            .lock()
            .unwrap()
            .tasks()
            .start(with_namespace!(req, ns))
            .await?;

        Ok(())
    }

    pub async fn kill_task(&self, cid: String, ns: &str) -> Result<(), Err> {
        let namespace = match ns {
            "" => NAMESPACE,
            _ => ns,
        };
        let mut c = self.client.lock().unwrap().tasks();
        let kill_request = KillRequest {
            container_id: cid.to_string(),
            signal: 15,
            all: true,
            ..Default::default()
        };
        c.kill(with_namespace!(kill_request, namespace))
            .await
            .expect("Failed to kill task");

        Ok(())
    }
    pub async fn pause_task() {
        todo!()
    }
    pub async fn resume_task() {
        todo!()
    }
    pub async fn delete_task(&self, cid: &str, ns: &str) {
        let namespace = match ns {
            "" => NAMESPACE,
            _ => ns,
        };
        let mut c = self.client.lock().unwrap().tasks();
        let time_out = Duration::from_secs(30);
        let wait_result = timeout(time_out, async {
            let wait_request = WaitRequest {
                container_id: cid.to_string(),
                ..Default::default()
            };

            let _ = c.wait(with_namespace!(wait_request, namespace)).await?;
            Ok::<(), Err>(())
        })
        .await;

        let kill_request = KillRequest {
            container_id: cid.to_string(),
            signal: 15,
            all: true,
            ..Default::default()
        };
        c.kill(with_namespace!(kill_request, namespace))
            .await
            .expect("Failed to kill task");

        match wait_result {
            Ok(Ok(_)) => {
                let req = DeleteTaskRequest {
                    container_id: cid.to_string(),
                };

                let _resp = c
                    .delete(with_namespace!(req, namespace))
                    .await
                    .expect("Failed to delete task");
                println!("Task: {:?} deleted", cid);
            }
            _ => {
                let kill_request = KillRequest {
                    container_id: cid.to_string(),
                    signal: 9,
                    all: true,
                    ..Default::default()
                };
                c.kill(with_namespace!(kill_request, namespace))
                    .await
                    .expect("Failed to FORCE kill task");
            }
        }
    }

    pub async fn get_container_list(&self, ns: &str) -> Result<Vec<String>, tonic::Status> {
        let namespace = match ns {
            "" => NAMESPACE,
            _ => ns,
        };
        let mut c = self.client.lock().unwrap().containers();

        let request = ListContainersRequest {
            ..Default::default()
        };

        let request = with_namespace!(request, namespace);

        let response = c.list(request).await?;

        Ok(response
            .into_inner()
            .containers
            .into_iter()
            .map(|container| container.id)
            .collect())
    }

    pub async fn get_task_list() {
        todo!()
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

    async fn handle_index(&self, data: &Vec<u8>, ns: &str) -> Option<ImageConfiguration> {
        let image_index: ImageIndex = ::serde_json::from_slice(&data).unwrap();
        let img_manifest_dscr = image_index
            .manifests()
            .iter()
            .find(|manifest_entry| match manifest_entry.platform() {
                Some(p) => {
                    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                    {
                        matches!(p.architecture(), &Arch::Amd64) && matches!(p.os(), &Os::Linux)
                    }
                    #[cfg(target_arch = "aarch64")]
                    {
                        matches!(p.architecture(), &Arch::ARM64) && matches!(p.os(), Os::Linux)
                        //&& matches!(p.variant().as_ref().map(|s| s.as_str()), Some("v8"))
                    }
                }
                None => false,
            })
            .unwrap();

        let req = ReadContentRequest {
            digest: img_manifest_dscr.digest().to_owned(),
            offset: 0,
            size: 0,
        };

        let mut c = self.client.lock().unwrap().content();
        let resp = c
            .read(with_namespace!(req, ns))
            .await
            .expect("Failed to read content")
            .into_inner()
            .message()
            .await
            .expect("Failed to read content message")
            .unwrap()
            .data;

        self.handle_manifest(&resp, ns).await
    }

    async fn handle_manifest(&self, data: &Vec<u8>, ns: &str) -> Option<ImageConfiguration> {
        let img_manifest: ImageManifest = ::serde_json::from_slice(&data).unwrap();
        let img_manifest_dscr = img_manifest.config();

        let req = ReadContentRequest {
            digest: img_manifest_dscr.digest().to_owned(),
            offset: 0,
            size: 0,
        };
        let mut c = self.client.lock().unwrap().content();

        let resp = c
            .read(with_namespace!(req, ns))
            .await
            .unwrap()
            .into_inner()
            .message()
            .await
            .unwrap()
            .unwrap()
            .data;

        ::serde_json::from_slice(&resp).unwrap()
    }

    pub async fn get_img_config(&self, name: &str, ns: &str) -> Option<ImageConfiguration> {
        let mut c = self.client.lock().unwrap().images();

        let req = GetImageRequest {
            name: name.to_string(),
        };
        let resp = c
            .get(with_namespace!(req, ns))
            .await
            .map_err(|e| {
                eprintln!(
                    "Failed to get the config of {} in namespace {}: {}",
                    name, ns, e
                );
                e
            })
            .ok()?
            .into_inner();

        let img_dscr = resp.image?.target?;
        let media_type = MediaType::from(img_dscr.media_type.as_str());

        let req = ReadContentRequest {
            digest: img_dscr.digest,
            ..Default::default()
        };

        let mut c = self.client.lock().unwrap().content();

        let resp = c
            .read(with_namespace!(req, ns))
            .await
            .map_err(|e| {
                eprintln!(
                    "Failed to read content for {} in namespace {}: {}",
                    name, ns, e
                );
                e
            })
            .ok()?
            .into_inner()
            .message()
            .await
            .map_err(|e| {
                eprintln!(
                    "Failed to read message for {} in namespace {}: {}",
                    name, ns, e
                );
                e
            })
            .ok()?
            .ok_or_else(|| {
                eprintln!("No data found for {} in namespace {}", name, ns);
                std::io::Error::new(std::io::ErrorKind::NotFound, "No data found")
            })
            .ok()?
            .data;

        let img_config = match media_type {
            MediaType::ImageIndex => self.handle_index(&resp, ns).await.unwrap(),
            MediaType::ImageManifest => self.handle_manifest(&resp, ns).await.unwrap(),
            MediaType::Other(media_type) => match media_type.as_str() {
                "application/vnd.docker.distribution.manifest.list.v2+json" => {
                    self.handle_index(&resp, ns).await.unwrap()
                }
                "application/vnd.docker.distribution.manifest.v2+json" => {
                    self.handle_manifest(&resp, ns).await.unwrap()
                }
                _ => {
                    eprintln!("Unexpected media type '{}'", media_type);
                    return None;
                }
            },
            _ => {
                eprintln!("Unexpected media type '{}'", media_type);
                return None;
            }
        };
        Some(img_config)
    }

    async fn get_parent_snapshot(&self, name: &str, ns: &str) -> Result<String, Err> {
        let img_config = self.get_img_config(name, ns).await.unwrap();

        let mut iter = img_config.rootfs().diff_ids().iter();
        let mut ret = iter
            .next()
            .map_or_else(String::new, |layer_digest| layer_digest.clone());

        while let Some(layer_digest) = iter.next() {
            let mut hasher = Sha256::new();
            hasher.update(ret.as_bytes());
            ret.push_str(&format!(",{}", layer_digest));
            hasher.update(" ");
            hasher.update(layer_digest);
            let digest = ::hex::encode(hasher.finalize());
            ret = format!("sha256:{digest}");
        }
        Ok(ret)
    }
}
//容器是容器，要先启动，然后才能运行任务
//要想删除一个正在运行的Task，必须先kill掉这个task，然后才能删除。