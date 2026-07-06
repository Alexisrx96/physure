from typing import Any

try:
    import torch
except ImportError:
    torch = None

try:
    import triton
    import triton.language as tl

    HAS_TRITON = True

    @triton.jit
    def covariance_update_kernel(
        sigma_ptr,
        jac_ptr,
        out_ptr,
        n_elements,
        block_size: tl.constexpr,
    ):
        """Computes Out = Jac * Sigma * Jac^T for diagonal Jac.

        Formula: Out[i, j] = Jac[i] * Sigma[i, j] * Jac[j].
        """
        pid_m = tl.program_id(axis=0)
        pid_n = tl.program_id(axis=1)

        # Range of indices handled by this block
        offs_am = (pid_m * block_size + tl.arange(0, block_size)) % n_elements
        offs_bn = (pid_n * block_size + tl.arange(0, block_size)) % n_elements

        # Load Jacobian values for rows (i) and cols (j)
        jac_m = tl.load(
            jac_ptr + offs_am, mask=offs_am < n_elements, other=0.0
        )
        jac_n = tl.load(
            jac_ptr + offs_bn, mask=offs_bn < n_elements, other=0.0
        )

        # Compute pointers for the block
        sig_ptrs = sigma_ptr + (
            offs_am[:, None] * n_elements + offs_bn[None, :]
        )

        # Load 2D block of Sigma
        sigma_block = tl.load(
            sig_ptrs,
            mask=(offs_am[:, None] < n_elements)
            & (offs_bn[None, :] < n_elements),
            other=0.0,
        )

        # Compute: jac[i] * sigma[i,j] * jac[j]
        out_block = jac_m[:, None] * sigma_block * jac_n[None, :]

        # Store result
        out_ptrs = out_ptr + (offs_am[:, None] * n_elements + offs_bn[None, :])
        tl.store(
            out_ptrs,
            out_block,
            mask=(offs_am[:, None] < n_elements)
            & (offs_bn[None, :] < n_elements),
        )

    # ponytail: fallback stub for the ImportError branch below has the
    # same name by design (optional-dependency pattern); not a real
    # redeclaration bug.
    def apply_covariance_update_triton(  # pyright: ignore[reportRedeclaration]
        sigma: Any, jac: Any
    ) -> Any:
        """Applies J * Sigma * J^T assuming J is diagonal (vector)."""
        assert sigma.is_cuda
        assert jac.is_cuda
        n = sigma.shape[0]
        out = torch.empty_like(sigma)

        def grid(meta):
            return (
                triton.cdiv(n, meta["block_size"]),
                triton.cdiv(n, meta["block_size"]),
            )

        covariance_update_kernel[grid](
            sigma,
            jac,
            out,
            n,
            block_size=32,  # pyright: ignore[reportArgumentType]
        )
        return out

except ImportError:
    # ponytail: HAS_TRITON toggles between True/False across the
    # try/except branches by design; not a real constant-redefinition bug.
    HAS_TRITON = False  # pyright: ignore[reportConstantRedefinition]

    def apply_covariance_update_triton(_sigma: Any, _jac: Any) -> Any:
        """Stub that fails loudly when Triton is unavailable."""
        raise RuntimeError("Triton kernels are not available on this system.")
