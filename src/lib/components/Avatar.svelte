<script lang="ts">
  import { avatarColor, avatarInitials } from "../format";
  let { name, size = 40 }: { name: string; size?: number } = $props();
  const initials = $derived(avatarInitials(name));
  const bg = $derived(avatarColor(name));
</script>

<div
  class="avatar"
  style="--size:{size}px; background:{bg};"
>
  <span>{initials}</span>
</div>

<style>
  .avatar {
    width: var(--size);
    height: var(--size);
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: white;
    font-weight: 600;
    font-size: calc(var(--size) * 0.4);
    user-select: none;
    flex-shrink: 0;
  }
</style>
