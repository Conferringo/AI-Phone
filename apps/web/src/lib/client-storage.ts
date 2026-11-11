/** Upgrade cleanup only; no retired feature is loaded or registered. */
export function retireClientStorage(storage:Pick<Storage,'removeItem'>){
 try{
  storage.removeItem('ai-phone.pet.preferences.v1');
  storage.removeItem('ai-phone.pet.position.v1');
 }catch{/* Storage restrictions must not prevent opening the workbench. */}
}
