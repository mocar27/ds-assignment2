// Register process jest po to, ze jak masz te funkcje w lib.rs run_register_process, 
// to pewnie chcialbys miec jakis stan/zmienne/cos i ta funkcja moze urosnac porzadnie, 
// wiec ja to sobie podzielilem na structa, wiec robie cos typu
// let rp = RegisterProcess::new(…);
// rp.run().await()