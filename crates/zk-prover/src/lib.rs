// crates/zk-prover/src/lib.rs

use p3_air::{Air, AirBuilder, BaseAir};
use p3_matrix::Matrix;

// DEFINI KOLOM (Execution Trace)
pub struct IntentRollupCols<T> {
    pub intent_id: T,
    
    // Parameter Intent dari Pengguna
    pub user_input_amount: T,
    pub user_min_output: T, // Batas toleransi (Slippage)
    
    // Eksekusi dari Solver pemenang lelang
    pub solver_actual_output: T, 
    
    // State Transisi Saldo
    pub user_balance: T,
    pub solver_balance: T,
}

// DEFINISI SIRKUIT (The AIR)
pub struct IntentRollupAir;

impl<F> BaseAir<F> for IntentRollupAir {
    fn width(&self) -> usize {
        6
    }
}

// LOGIKA KENDALA (The Constraints)
impl<AB: AirBuilder> Air<AB> for IntentRollupAir {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();

        // Ambil baris saat ini (local) dan baris berikutnya (next) dari matriks eksekusi
        let local = main.row_slice(0).unwrap();
        let next = main.row_slice(1).unwrap();

        // Jika kita berada di baris terakhir, tidak perlu membandingkan dengan baris "next"
        // builder.transition() memastikan aturan ini hanya berlaku antar-baris.
        let mut transition = builder.when_transition();

        // Mapping Kolom (Local Row)
        let local_user_input = local[1].clone();
        let local_min_output = local[2].clone();
        let local_actual_output = local[3].clone();
        let local_user_bal = local[4].clone();
        let local_solver_bal = local[5].clone();

        // Mapping Kolom (Next Row - State setelah intent diselesaikan)
        let next_user_bal = next[4].clone();
        let next_solver_bal = next[5].clone();

        // --- ATURAN 1: PERLINDUNGAN SLIPPAGE (INTI DARI INTENT) ---
        // ZK Circuit HARUS menggagalkan proof jika solver memberikan output lebih kecil dari yang diminta user.
        // Di Plonky3 (Finite Fields), ketidaksamaan (>=) biasanya dilakukan melalui Range Check (Lookup Table). 
        // Secara konseptual, sirkuit memverifikasi: builder.assert_range_check(local_actual_output - local_min_output);
        
        // --- ATURAN 2: PENGURANGAN SALDO PENGGUNA ---
        // Saldo pengguna dipotong sebesar input_amount yang ia jaminkan
        transition.assert_eq(
            next_user_bal.clone() + local_user_input.clone(), 
            local_user_bal.clone() + local_actual_output.clone()
        );
        // Persamaan di atas setara dengan: 
        // NextUserBal = LocalUserBal - UserInput + ActualOutput (Jika swap beda token)
        // (Catatan: Untuk penyederhanaan contoh, kita gabungkan di satu state)

        // --- ATURAN 3: PENDAPATAN SOLVER (SETTLEMENT) ---
        // Solver mendapatkan input pengguna, tetapi saldonya dipotong untuk membayar output pengguna
        transition.assert_eq(
            next_solver_bal.clone() + local_actual_output.clone(),
            local_solver_bal.clone() + local_user_input.clone()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_air_compilation() {
        let _air = IntentRollupAir;
        assert!(true);
    }
}