//! The UI's own pure data types: what the list shows, and what draws the windows.

use serde::{Deserialize, Serialize};

use crate::model::Category;

/// A node of the Categories tree, what the list shows (saved: the last one comes back).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum View {
    All,
    Cat(Category),
    Unfinished(Option<Category>),
    Finished(Option<Category>),
    /// A queue (index into the app's `App::queues`).
    Queue(usize),
}

/// A choice of View → Draw with, as saved in the settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RendererApi {
    #[serde(rename = "opengl")]
    OpenGl,
    #[serde(rename = "direct3d")]
    Direct3D,
    #[serde(rename = "vulkan")]
    Vulkan,
    #[serde(rename = "metal")]
    Metal,
}

impl RendererApi {
    /// Its name in the log and the crash reports (the menu's are translated texts, app.rs).
    pub fn name(self) -> &'static str {
        match self {
            RendererApi::OpenGl => "OpenGL",
            RendererApi::Direct3D => "Direct3D 12",
            RendererApi::Vulkan => "Vulkan",
            RendererApi::Metal => "Metal",
        }
    }

    /// The first part of its files' names in the data folder (`direct3d-failed.txt`…).
    pub fn key(self) -> &'static str {
        match self {
            RendererApi::OpenGl => "opengl",
            RendererApi::Direct3D => "direct3d",
            RendererApi::Vulkan => "vulkan",
            RendererApi::Metal => "metal",
        }
    }
}
