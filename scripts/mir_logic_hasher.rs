// Experimental helper for MIR/type-4 clone research.
// Not wired into the supported analysis flow.
// Requires a nightly toolchain plus rustc_private/rustc-dev components.

#![feature(rustc_private)]

extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_hir;

use rustc_driver::Compilation;
use rustc_interface::interface;
use rustc_middle::mir::visit::Visitor;
use rustc_middle::mir::{Body, Statement, Terminator, Operand, Rvalue};
use rustc_middle::ty::TyCtxt;
use rustc_session::config;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

struct MirLogicHasher<'tcx> {
    tcx: TyCtxt<'tcx>,
    hasher: DefaultHasher,
}

impl<'tcx> Visitor<'tcx> for MirLogicHasher<'tcx> {
    fn visit_statement(&mut self, statement: &Statement<'tcx>, _location: rustc_middle::mir::Location) {
        // Hash the "kind" of statement, ignoring specific variable indices/names
        // This detects if the same sequence of operations (Assign, SetDiscriminant, etc.) occurs
        std::mem::discriminant(&statement.kind).hash(&mut self.hasher);
        self.super_statement(statement, _location);
    }

    fn visit_terminator(&mut self, terminator: &Terminator<'tcx>, _location: rustc_middle::mir::Location) {
        // Hash control flow structures (Goto, SwitchInt, Call, Return)
        std::mem::discriminant(&terminator.kind).hash(&mut self.hasher);
        self.super_terminator(terminator, _location);
    }

    fn visit_rvalue(&mut self, rvalue: &Rvalue<'tcx>, _location: rustc_middle::mir::Location) {
        // Hash the operations (BinaryOp, UnaryOp, Cast, etc.)
        std::mem::discriminant(rvalue).hash(&mut self.hasher);
        self.super_rvalue(rvalue, _location);
    }

    fn visit_operand(&mut self, operand: &Operand<'tcx>, _location: rustc_middle::mir::Location) {
        // Hash constants (literals), but potentially normalize them if we want Type-3/4
        if let Operand::Constant(c) = operand {
            "CONST".hash(&mut self.hasher);
        }
        self.super_operand(operand, _location);
    }
}

struct CloneCallback;

impl rustc_driver::Callbacks for CloneCallback {
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &interface::Compiler,
        queries: &'tcx rustc_interface::Queries<'tcx>,
    ) -> Compilation {
        queries.global_ctxt().unwrap().enter(|tcx| {
            // Iterate over all functions defined in the local crate
            for def_id in tcx.hir().body_owners() {
                let local_def_id = def_id.to_def_id().expect_local();
                let body = tcx.optimized_mir(local_def_id);
                
                let mut hasher = MirLogicHasher {
                    tcx,
                    hasher: DefaultHasher::new(),
                };
                hasher.visit_body(body);
                
                let logic_hash = hasher.hasher.finish();
                let name = tcx.item_name(local_def_id.to_def_id());
                
                println!("Function: {:<20} | Logic Hash: {:x}", name, logic_hash);
            }
        });
        Compilation::Stop
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut rustc_args = args.clone();
    
    // Minimal arguments to get rustc to run analysis
    rustc_args.push("-Z".to_string());
    rustc_args.push("allow-features=rustc_private".to_string());

    rustc_driver::RunCompiler::new(&rustc_args, &mut CloneCallback).run().unwrap();
}
