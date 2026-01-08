import torch
import triton
import triton.language as tl


@triton.jit
def covariance_update_kernel(
    sigma_ptr,
    jac_ptr,
    out_ptr,
    n_elements,
    BLOCK_SIZE: tl.constexpr,
):
    """Computes Out = Jac * Sigma * Jac^T where Jac is diagonal (represented by vector).
    Formula: Out[i, j] = Jac[i] * Sigma[i, j] * Jac[j]
    """
    pid_m = tl.program_id(axis=0)
    pid_n = tl.program_id(axis=1)

    # Range of indices handled by this block
    offs_am = (pid_m * BLOCK_SIZE + tl.arange(0, BLOCK_SIZE)) % n_elements
    offs_bn = (pid_n * BLOCK_SIZE + tl.arange(0, BLOCK_SIZE)) % n_elements

    # Load Jacobian values for rows (i) and cols (j)
    jac_m = tl.load(jac_ptr + offs_am, mask=offs_am < n_elements, other=0.0)
    jac_n = tl.load(jac_ptr + offs_bn, mask=offs_bn < n_elements, other=0.0)

    # Load Sigma block
    # Sigma is N x N (flattened or stride handling needed)
    # We assume row-major storage for now
    # Pointer arithmetic: ptr + row * N + col
    # But here we have block of rows and block of cols.
    # We need to load a 2D block.

    # Compute pointers for the block
    sig_ptrs = sigma_ptr + (offs_am[:, None] * n_elements + offs_bn[None, :])

    # Load 2D block of Sigma
    sigma_block = tl.load(
        sig_ptrs,
        mask=(offs_am[:, None] < n_elements) & (offs_bn[None, :] < n_elements),
        other=0.0,
    )

    # Compute: jac[i] * sigma[i,j] * jac[j]
    out_block = jac_m[:, None] * sigma_block * jac_n[None, :]

    # Store result
    out_ptrs = out_ptr + (offs_am[:, None] * n_elements + offs_bn[None, :])
    tl.store(
        out_ptrs,
        out_block,
        mask=(offs_am[:, None] < n_elements) & (offs_bn[None, :] < n_elements),
    )


def apply_covariance_update_triton(
    sigma: torch.Tensor, jac: torch.Tensor
) -> torch.Tensor:
    """Applies J * Sigma * J^T assuming J is diagonal (vector)."""
    assert sigma.is_cuda and jac.is_cuda
    n = sigma.shape[0]
    out = torch.empty_like(sigma)

    grid = lambda META: (
        triton.cdiv(n, META["BLOCK_SIZE"]),
        triton.cdiv(n, META["BLOCK_SIZE"]),
    )

    covariance_update_kernel[grid](
        sigma,
        jac,
        out,
        n,
        BLOCK_SIZE=32,
    )
    return out
