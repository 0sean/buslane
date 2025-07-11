class AddNocIndexToAgency < ActiveRecord::Migration[8.0]
  def change
    add_index :agencies, :noc
  end
end
