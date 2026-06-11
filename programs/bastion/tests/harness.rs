mod helpers;

use anchor_litesvm::Report;
use bastion::utils::helpers::{derive_policy, BastionBundle, SessionRoot};
use helpers::*;
use solana_keypair::Keypair;
use solana_signer::Signer;

#[test]
fn setup_svm_loads_program_and_funds_payer() {
    let mut md = Report::new(
        "Bastion: the context loads the program and funds a cast actor",
        "`bastion_ctx()` loads bastion.so and pins the clock; casting an actor \
         airdrops it 100 SOL. This guards the plumbing the scenarios stand on.",
    );
    let mut ctx = bastion_ctx();

    md.step("Cast an owner: a freshly-cast actor is airdropped 100 SOL");
    let owner = ctx.cast_actor("owner");
    let bal = ctx.svm.get_balance(&owner.pubkey()).unwrap();

    md.check("cast actor funded with 100 SOL", 100 * ONE_SOL, bal);
    ctx.report_execution(&mut md);
}

#[test]
fn pda_derivations_are_deterministic() {
    let mut md = Report::new(
        "Bastion: the bundle's PDA derivations are deterministic",
        "The bundle projects the session, delegate, and policy PDAs from a \
         `SessionRoot`. Same roots derive the same addresses; a different \
         session key derives a different session; successive policy slots differ.",
    );
    let ctx = bastion_ctx();

    let owner = Keypair::new();
    let session_kp = Keypair::new();
    let root = SessionRoot {
        owner: owner.pubkey(),
        session_key: session_kp.pubkey(),
    };

    md.step("Project the bundle twice from identical roots");
    let b1 = BastionBundle::from(&root);
    let b2 = BastionBundle::from(&root);
    md.check("session PDA is deterministic", b1.session, b2.session);
    md.check("delegate PDA is deterministic", b1.delegate, b2.delegate);

    md.step("Project from a different session key");
    let other_kp = Keypair::new();
    let b3 = BastionBundle::from(&SessionRoot {
        owner: owner.pubkey(),
        session_key: other_kp.pubkey(),
    });
    md.check(
        "a different session key derives a different session",
        true,
        b1.session != b3.session,
    );

    md.step("Derive successive policy slots for one session");
    let p0 = derive_policy(b1.session, 0);
    let p1 = derive_policy(b1.session, 1);
    md.check("policy slots 0 and 1 differ", true, p0 != p1);

    let _ = ctx;
    ctx.report_execution(&mut md);
}
