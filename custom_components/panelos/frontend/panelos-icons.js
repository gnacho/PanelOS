// The PanelOS mark (P in Morse, dot dash / dash dot) as the icon "panelos:mark",
// for the sidebar and anywhere else in the frontend.
const PANELOS_MARK =
  "M2 8a2 2 0 1 0 4 0a2 2 0 1 0-4 0zM12 6h8a2 2 0 0 1 0 4h-8a2 2 0 0 1 0-4z" +
  "M4 14h8a2 2 0 0 1 0 4H4a2 2 0 0 1 0-4zM18 16a2 2 0 1 0 4 0a2 2 0 1 0-4 0z";

window.customIcons = window.customIcons || {};
window.customIcons.panelos = {
  getIcon: async (name) => (name === "mark" ? { path: PANELOS_MARK } : undefined),
  getIconList: async () => [{ name: "mark" }],
};
