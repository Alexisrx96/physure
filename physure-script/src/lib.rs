pub mod ast;
pub mod debug;
pub mod decorators;
pub mod inspect;
pub mod lexer;
pub mod module;
pub mod parser;
pub mod pipeline;
pub mod resolver;
pub mod interpreter;
pub mod codegen;
pub mod exporter;
pub mod value;
pub mod symbolic;
pub mod builtins;
pub mod plugin;

pub use codegen::{transpile, Target};
pub use ast::{Expr, Program, Statement};
pub use debug::{DebugAction, DebugContext, DebugHook, StackFrame};
pub use lexer::{PhsLexer, PhsToken, TokenKind};
pub use parser::{parse_phs, parse_phs_with_lines};
pub use interpreter::{eval_phs, ExternalFn, PhsInterpreter};
pub use module::{ComposedFunction, FunctionSignature, ParamInfo, PhsModule};
pub use pipeline::{PhsPipeline, PipelineArg, PipelineStep};
pub use value::{PhsValue, PlotData};
pub use plugin::{
    PluginEntryPoint, PluginFn, PluginFnEntry, PluginRegistry, PluginState, PluginValue,
    PluginValueTag, PHS_PLUGIN_ABI_VERSION,
};
