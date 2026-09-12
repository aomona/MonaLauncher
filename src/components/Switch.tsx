import { Switch as BaseSwitch } from "@base-ui/react/switch";

export function Switch(
  props: Omit<BaseSwitch.Root.Props, "render" | "nativeButton" | "className">,
) {
  return (
    <BaseSwitch.Root
      {...props}
      nativeButton
      render={
        <button
          type="button"
          aria-labelledby={props["aria-labelledby"]}
          aria-label={props["aria-label"]}
        />
      }
      className="switch"
    >
      <span className="switch-track" aria-hidden="true">
        <BaseSwitch.Thumb className="switch-thumb" />
      </span>
    </BaseSwitch.Root>
  );
}
